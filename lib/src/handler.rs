use std::fmt;

use futures::Future;
use hyper;

use response::{Response};
use context::Context;
use StatusCode;
use ext::{FutureExt, IntoFutureExt, BoxFuture};

#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value, use_debug))]
pub(crate) fn default_catch<E: fmt::Debug + Send>(err: E) -> Response {
    // TODO: Support definable error catchers
    /*
        Idea of syntax:

        Shio::new().catch(|err: Error| { ... })
    */

    // Default "error catcher" just logs with error! and responds with a 500
    error!("{:?}", err);

    Response::build()
        .status(StatusCode::InternalServerError)
        .into()
}

pub trait Handler: Send + Sync
where
    <Self::Result as IntoFutureExt<Response>>::Error: fmt::Debug + Send + Sync,
{
    type Result: IntoFutureExt<Response>;

    fn call(&self, context: Context) -> Self::Result;

    #[inline]
    fn into_box(self) -> BoxHandler
    where
        Self: Sized + 'static,
    {
        Box::new(move |ctx: Context| -> BoxFuture<Response, hyper::Error> {
            self.call(ctx)
                .into_future_ext()
                .or_else(default_catch)
                .into_box()
        })
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub type BoxHandler = Box<Handler<Result = BoxFuture<Response, hyper::Error>>>;

impl<TError, TFuture, TFn> Handler for TFn
where
    TError: fmt::Debug + Send + Sync,
    TFuture: IntoFutureExt<Response, Error = TError>,
    TFn: Send + Sync,
    TFn: Fn(Context) -> TFuture,
{
    type Result = TFuture;

    #[inline]
    fn call(&self, context: Context) -> Self::Result {
        (*self)(context)
    }
}
