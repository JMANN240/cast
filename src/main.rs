use clap::Parser;

use axum::{
	body::Body,
	extract::connect_info::{self, ConnectInfo},
	http::{Method, Request, StatusCode},
	routing::get,
	Router,
};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper_util::{
	rt::{TokioExecutor, TokioIo},
	server,
};
use std::{convert::Infallible, path::PathBuf, sync::Arc};
use tokio::net::{unix::UCred, UnixListener, UnixStream};
use tower::Service;

#[derive(Parser)]
struct Args {
	#[arg(long, value_name = "PATH")]
	uds: String
}

#[tokio::main]
async fn main() {
	let args = Args::parse();

	let uds_path = PathBuf::from(args.uds);

	let _ = tokio::fs::remove_file(&uds_path).await;
	tokio::fs::create_dir_all(uds_path.parent().unwrap()).await.unwrap();

	let uds = UnixListener::bind(uds_path.clone()).unwrap();
	println!("3");
	let handle = tokio::spawn(async move {
		println!("1");
		let app = Router::new()
			.route("/", get(root));

		let mut make_service = app.into_make_service_with_connect_info::<UdsConnectInfo>();

		loop {
			let (socket, _remote_addr) = uds.accept().await.unwrap();
			println!("5");
			let tower_service = make_service.call(&socket).await.unwrap();

			tokio::spawn(async move {
				let socket = TokioIo::new(socket);

				let hyper_service = hyper::service::service_fn(move |request: axum::http::Request<Incoming>| {
					tower_service.clone().call(request)
				});

				if let Err(err) = server::conn::auto::Builder::new(TokioExecutor::new())
					.serve_connection_with_upgrades(socket, hyper_service)
					.await
				{
					eprintln!("failed to serve connection: {err:#}");
				}
			});
		}
	});
	handle.await.unwrap();
	println!("4");
}

async fn root() -> &'static str {
	"Hello, Rust Axum Tokio UDS!"
}

#[derive(Clone, Debug)]
struct UdsConnectInfo {
	peer_addr: Arc<tokio::net::unix::SocketAddr>,
	peer_cred: UCred
}

impl connect_info::Connected<&UnixStream> for UdsConnectInfo {
	fn connect_info(target: &UnixStream) -> Self {
		let peer_addr = target.peer_addr().unwrap();
		let peer_cred = target.peer_cred().unwrap();

		Self {
			peer_addr: Arc::new(peer_addr),
			peer_cred,
		}
	}
}