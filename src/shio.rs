use std::sync::Arc;
use std::thread;
use std::net::SocketAddr;

use num_cpus;
use futures::{future, Stream};
use hyper::server::Http;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use net2::TcpBuilder;

#[cfg(unix)]
use net2::unix::UnixTcpBuilderExt;

use handler::Handler;
use router::{Route, Router};
use errors::ListenError;
use util::ToSocketAddrsExt;
use service::Service;

pub struct Shio<H: Handler + 'static> {
    handler: Arc<H>,
    threads: usize,
}

impl<H: Handler> Shio<H> {
    pub fn new(handler: H) -> Self {
        Shio {
            handler: Arc::new(handler),
            threads: num_cpus::get(),
        }
    }

    /// Set the number of threads to use.
    pub fn threads(&mut self, threads: usize) {
        self.threads = threads;
    }

    pub fn run<A: ToSocketAddrsExt>(&self, addr: A) -> Result<(), ListenError> {
        let addrs = addr.to_socket_addrs_ext()?.collect::<Vec<_>>();
        let mut children = Vec::new();

        for _ in 0..self.threads {
            let addrs = addrs.clone();
            let handler = self.handler.clone();

            children.push(thread::spawn(move || -> Result<(), ListenError> {
                let mut core = Core::new()?;
                let mut work = Vec::new();
                let handle = core.handle();
                let service = Service::new(handler, handle.clone());

                for addr in &addrs {
                    let handle = handle.clone();
                    let builder = (match *addr {
                        SocketAddr::V4(_) => TcpBuilder::new_v4(),
                        SocketAddr::V6(_) => TcpBuilder::new_v6(),
                    })?;

                    // Set SO_REUSEADDR on the socket
                    builder.reuse_address(true)?;

                    // Set SO_REUSEPORT on the socket (in unix)
                    #[cfg(unix)]
                    builder.reuse_port(true)?;

                    builder.bind(&addr)?;

                    let listener = TcpListener::from_listener(
                        // TODO: Should this be configurable somewhere?
                        builder.listen(128)?,
                        addr,
                        &handle,
                    )?;

                    let protocol = Http::new();
                    let service = service.clone();

                    let srv = listener.incoming().for_each(move |(socket, addr)| {
                        protocol.bind_connection(&handle, socket, addr, service.clone());

                        Ok(())
                    });

                    work.push(srv);
                }

                core.run(future::join_all(work))?;

                Ok(())
            }));
        }

        for child in children.drain(..) {
            child.join().unwrap()?;
        }

        Ok(())
    }
}

impl Default for Shio<Router> {
    fn default() -> Self {
        Shio::new(Router::new())
    }
}

impl Shio<Router> {
    pub fn route<R: Into<Route>>(&mut self, route: R) -> &mut Self {
        Arc::get_mut(&mut self.handler).map(|router| router.route(route));

        self
    }
}
