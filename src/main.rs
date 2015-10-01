extern crate mio;
extern crate bytes;

use mio::*;
use mio::tcp::*;
use bytes::{ByteBuf, MutByteBuf};
use mio::util::Slab;
use std::io;
use std::net::{SocketAddr};
use std::str::FromStr;

const SERVER: Token = Token(0);
type MyEventLoop = EventLoop<EchoServer>;

struct EchoConn {
    sock: TcpStream,
    buf: Option<ByteBuf>,
    mut_buf: Option<MutByteBuf>,
    token: Option<Token>,
    interest: EventSet
}

impl EchoConn {
    fn new(sock: TcpStream) -> EchoConn {
        EchoConn {
            sock: sock,
            buf: None,
            mut_buf: Some(ByteBuf::mut_with_capacity(2048)),
            token: None,
            interest: EventSet::hup()
        }
    }

    fn writable(&mut self, event_loop: &mut MyEventLoop) -> io::Result<()> {
        let mut buf = self.buf.take().unwrap();

        match self.sock.try_write_buf(&mut buf) {
            Ok(None) => {
                self.buf = Some(buf);
                self.interest.insert(EventSet::writable());
            }
            Ok(Some(r)) => {
                let remaining = buf.remaining ();

                if remaining > 0 {
                    println!("we wrote {} bytes but {} remaining!", r, remaining);
                    self.interest.insert(EventSet::writable());
                }
                else {
                    println!("we wrote {} bytes, done", r);
                    self.mut_buf = Some(buf.flip());
                    self.interest.insert(EventSet::readable());
                    self.interest.remove(EventSet::writable());
                }
            }
            Err(e) => println!("not implemented; client err={:?}", e),
        }

        event_loop.reregister(&self.sock, self.token.unwrap(), self.interest,
                              PollOpt::edge() | PollOpt::oneshot())
    }

    fn readable(&mut self, event_loop: &mut MyEventLoop) -> io::Result<()> {
        let mut buf = self.mut_buf.take().unwrap();

        match self.sock.try_read_buf(&mut buf) {
            Ok(None) => {
                panic!("we just got readable, but were unable to read from the socket?");
            }
            Ok(Some(r)) => {
                println!("we read {} bytes!", r);
                if r == 0 {
                    println!("{:?}: end of connection", self.token.unwrap ());
                    self.interest.remove(EventSet::readable());
                }
                else {
                    self.interest.remove(EventSet::readable());
                    self.interest.insert(EventSet::writable());
 
                }
            }
            Err(e) => {
                println!("not implemented; client err={:?}", e);
                self.interest.remove(EventSet::readable());
            }
        };
        
        // prepare to provide this to writable
        self.buf = Some(buf.flip());
        event_loop.reregister(&self.sock, self.token.unwrap(), self.interest, PollOpt::edge())
    }
}

struct EchoServer {
    sock: TcpListener,
    conns: Slab<EchoConn>
}

impl EchoServer {
    fn new(sock: TcpListener) -> EchoServer {
        EchoServer {
            sock: sock,
            conns: Slab::new_starting_at(Token(2), 128)
        }
    }

    fn accept(&mut self, event_loop: &mut MyEventLoop) -> io::Result<()> {
        println!("server accepting socket");

        let sock = self.sock.accept().unwrap().unwrap();
        let conn = EchoConn::new(sock);
        let tok = self.conns.insert(conn).ok().expect("could not add connection to slab");
        println! ("assigned token {:?}", tok);

        // Register the connection
        self.conns[tok].token = Some(tok);
        event_loop.register_opt(&self.conns[tok].sock, tok, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot())
            .ok().expect("could not register socket with event loop");

        Ok(())
    }

    fn conn_readable(&mut self, event_loop: &mut MyEventLoop, tok: Token) -> io::Result<()> {
        println!("server conn readable; tok={:?}", tok);
        self.conn(tok).readable(event_loop)
    }

    fn conn_writable(&mut self, event_loop: &mut MyEventLoop, tok: Token) -> io::Result<()> {
        println!("server conn writable; tok={:?}", tok);
        self.conn(tok).writable(event_loop)
    }

    fn conn<'a>(&'a mut self, tok: Token) -> &'a mut EchoConn {
        &mut self.conns[tok]
    }
}

impl Handler for EchoServer {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut MyEventLoop, token: Token, events: EventSet) {
        println!("ready {:?} {:?}", token, events);
        if events.is_readable() {
            match token {
                SERVER => self.accept(event_loop).unwrap(),
                i => self.conn_readable(event_loop, i).unwrap()
            }
        }

        if events.is_writable() {
            match token {
                SERVER => panic!("received writable for SERVER: impossible, can only accept connections"),
                _ => self.conn_writable(event_loop, token).unwrap()
            };
        }
    }
}

pub fn localhost() -> SocketAddr {
    FromStr::from_str("127.0.0.1:7000").unwrap()
}

pub fn main () {
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost ();
    let srv_sock = TcpListener::bind(&addr).unwrap();

    println!("listen for connections");
    event_loop.register_opt(
        &srv_sock, SERVER, 
        EventSet::readable(),
        PollOpt::edge() | PollOpt::oneshot()
    ).unwrap();

    // Start the event loop
    event_loop.run(&mut EchoServer::new(srv_sock)).unwrap();
}
