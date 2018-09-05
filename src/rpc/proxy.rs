use hyper::Client;
use hyper::header::HeaderValue;
use hyper::rt::Stream;
use hyper::{self, Body, Method, Request as HyperRequest};
use jsonrpc_core::*;
use std::collections::{HashMap, HashSet};
use std::default::Default;
use std::sync::Arc;
use web3::futures::Future;

#[derive(Debug, Default)]
pub struct ProxyHandler<M: Metadata = ()> {
    methods: HashMap<String, RemoteProcedure<M>>,
    proxy_target_url: String,
}

// Type inference helper
impl ProxyHandler {
    /// Creates new `ProxyHandler` without any metadata.
    pub fn new(proxy_target_url: &str) -> Self {
        Self {
            methods: HashMap::default(),
            proxy_target_url: proxy_target_url.to_owned(),
        }
    }
}

impl<M: Metadata + Default> ProxyHandler<M> {
    pub fn add_method<F>(&mut self, name: &str, method: F)
    where
        F: RpcMethodSimple,
    {
        self.methods.insert(
            name.to_owned(),
            RemoteProcedure::Method(Arc::new(move |params, _| method.call(params))),
        );
    }
}
impl From<ProxyHandler> for MetaIoHandler<(), ProxyMiddleware> {
    fn from(io: ProxyHandler) -> Self {
        let mut handler = MetaIoHandler::with_middleware(ProxyMiddleware::new(
            io.proxy_target_url,
            io.methods.clone().into_iter().map(|(key, _)| key).collect(),
        ));

        for (name, method) in io.methods.into_iter() {
            handler.add_method(&name, move |params| match method.clone() {
                RemoteProcedure::Method(method) => method.call(params, ()),
                _ => unimplemented!(),
            });
        }

        handler
    }
}

#[derive(Debug, Default)]
pub struct ProxyMiddleware {
    proxy_target_url: String,
    methods: HashSet<String>,
}

impl ProxyMiddleware {
    pub fn new(proxy_target_url: String, methods: HashSet<String>) -> Self {
        Self {
            proxy_target_url,
            methods,
        }
    }
}

impl<M: Metadata> Middleware<M> for ProxyMiddleware {
    type Future = Box<Future<Item = Option<Response>, Error = ()> + Send>;

    fn on_request<F, X>(&self, request: Request, meta: M, process: F) -> Self::Future
    where
        F: FnOnce(Request, M) -> X + Send,
        X: Future<Item = Option<Response>, Error = ()> + Send + 'static,
    {
        let mut matches_custom_method = false;
        if let Request::Single(Call::MethodCall(call)) = &request {
            debug!("RPC method: {}", &call.method);
            if self.methods.contains(&call.method) {
                matches_custom_method = true;
            }
        }

        if matches_custom_method {
            return Box::new(process(request, meta));
        }

        let client = Client::new();
        let uri: hyper::Uri = self.proxy_target_url.parse().unwrap();
        let proxy_payload = serde_json::to_string(&request).unwrap();

        let mut req = HyperRequest::new(Body::from(proxy_payload));
        *req.method_mut() = Method::POST;
        *req.uri_mut() = uri.clone();
        req.headers_mut().insert(
            "content-type",
            HeaderValue::from_str("application/json").unwrap(),
        );

        let post = client
            .request(req)
            .and_then(|res| res.into_body().concat2());

        Box::new(post.map_err(|_| ()).and_then(|body| {
            let response: Response = serde_json::from_slice(&body).unwrap();
            Ok(Some(response))
        }))
    }
}
