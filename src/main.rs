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

    fn writable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        let mut buf = self.buf.take().unwrap();

        match self.sock.try_write_buf(&mut buf) {
            Ok(None) => {
                print!("client flushing buf; WOULDBLOCK");

                self.buf = Some(buf);
                self.interest.insert(EventSet::writable());
            }
            Ok(Some(r)) => {
                print!("CONN : we wrote {} bytes!", r);

                self.mut_buf = Some(buf.flip());

                self.interest.insert(EventSet::readable());
                self.interest.remove(EventSet::writable());
            }
            Err(e) => print!("not implemented; client err={:?}", e),
        }

        event_loop.reregister(&self.sock, self.token.unwrap(), self.interest,
                              PollOpt::edge() | PollOpt::oneshot())
    }

    fn readable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        let mut buf = self.mut_buf.take().unwrap();

        match self.sock.try_read_buf(&mut buf) {
            Ok(None) => {
                panic!("We just got readable, but were unable to read from the socket?");
            }
            Ok(Some(r)) => {
                print!("CONN : we read {} bytes!", r);
                self.interest.remove(EventSet::readable());
                self.interest.insert(EventSet::writable());
            }
            Err(e) => {
                print!("not implemented; client err={:?}", e);
                self.interest.remove(EventSet::readable());
            }

        };

        // prepare to provide this to writable
        self.buf = Some(buf.flip());
        event_loop.reregister(&self.sock, self.token.unwrap(), self.interest,
                              PollOpt::edge())
    }
}

struct EchoServer {
    sock: TcpListener,
    conns: Slab<EchoConn>
}

impl EchoServer {
    fn accept(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        print!("server accepting socket");

        let sock = self.sock.accept().unwrap().unwrap();
        let conn = EchoConn::new(sock,);
        let tok = self.conns.insert(conn)
            .ok().expect("could not add connection to slab");

        // Register the connection
        self.conns[tok].token = Some(tok);
        event_loop.register_opt(&self.conns[tok].sock, tok, EventSet::readable(),
                                PollOpt::edge() | PollOpt::oneshot())
            .ok().expect("could not register socket with event loop");

        Ok(())
    }

    fn conn_readable(&mut self, event_loop: &mut EventLoop<Echo>,
                     tok: Token) -> io::Result<()> {
        print!("server conn readable; tok={:?}", tok);
        self.conn(tok).readable(event_loop)
    }

    fn conn_writable(&mut self, event_loop: &mut EventLoop<Echo>,
                     tok: Token) -> io::Result<()> {
        print!("server conn writable; tok={:?}", tok);
        self.conn(tok).writable(event_loop)
    }

    fn conn<'a>(&'a mut self, tok: Token) -> &'a mut EchoConn {
        &mut self.conns[tok]
    }
}

struct Echo {
    server: EchoServer,
}

impl Echo {
    fn new(sock: TcpListener) -> Echo {
        Echo {
            server: EchoServer {
                sock: sock,
                conns: Slab::new_starting_at(Token(2), 128)
            },
        }
    }
}

impl Handler for Echo {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Echo>, token: Token,
             events: EventSet) {
        print!("ready {:?} {:?}", token, events);
        if events.is_readable() {
            match token {
                SERVER => self.server.accept(event_loop).unwrap(),
                i => self.server.conn_readable(event_loop, i).unwrap()
            }
        }

        if events.is_writable() {
            match token {
                SERVER => panic!("received writable for token 0"),
                _ => self.server.conn_writable(event_loop, token).unwrap()
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

    print!("listen for connections");
    event_loop.register_opt(&srv_sock, SERVER, EventSet::readable(),
                            PollOpt::edge() | PollOpt::oneshot()).unwrap();

    // Start the event loop
    event_loop.run(&mut Echo::new(srv_sock)).unwrap();
}
