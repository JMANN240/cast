use clap::Parser;
use serde::{Deserialize, Serialize};
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
use serde_json::from_str;
use socketioxide::{SocketIo, extract::{SocketRef, Data}, socket::Sid};
use std::{convert::Infallible, path::PathBuf, sync::Arc, str::FromStr};
use tokio::net::{unix::UCred, UnixListener, UnixStream};
use tower::Service;
use tower_http::services::ServeDir;

#[derive(Parser)]
struct Args {
	#[arg(long, value_name = "PATH")]
	uds: String
}

#[derive(Debug, Deserialize, Serialize)]
struct Offer {
	to_id: String,
	from_id: String,
	offer: serde_json::Value
}

#[derive(Debug, Deserialize, Serialize)]
struct Answer {
	to_id: String,
	from_id: String,
	answer: serde_json::Value
}

#[derive(Debug, Deserialize, Serialize)]
struct Candidate {
	to_id: String,
	from_id: String,
	cnd: serde_json::Value
}

#[derive(Debug, Deserialize, Serialize)]
struct Watcher {
	to_id: String,
	from_id: String
}

#[tokio::main]
async fn main() {
	let args = Args::parse();

	let uds_path = PathBuf::from(args.uds);

	let _ = tokio::fs::remove_file(&uds_path).await;
	tokio::fs::create_dir_all(uds_path.parent().unwrap()).await.unwrap();

	let uds = UnixListener::bind(uds_path.clone()).unwrap();

	let (socketio_layer, socketio) = SocketIo::new_layer();

	let socket_socketio = socketio.clone();

	socketio.ns("/", move |socket: SocketRef| {
		let offer_socketio = socket_socketio.clone();

		socket.on("offer", move |Data::<Offer>(offer)| {
			let to_socket = offer_socketio.get_socket(Sid::from_str(&offer.to_id).unwrap()).unwrap();
			to_socket.emit("offer", offer).unwrap();
		});

		let answer_socketio = socket_socketio.clone();

		socket.on("answer", move |Data::<Answer>(answer)| {
			let to_socket = answer_socketio.get_socket(Sid::from_str(&answer.to_id).unwrap()).unwrap();
			to_socket.emit("answer", answer).unwrap();
		});

		let candidate_socketio = socket_socketio.clone();

		socket.on("candidate", move |Data::<Candidate>(candidate)| {
			let to_socket = candidate_socketio.get_socket(Sid::from_str(&candidate.to_id).unwrap()).unwrap();
			to_socket.emit("candidate", candidate).unwrap();
		});

		let watcher_socketio = socket_socketio.clone();

		socket.on("watcher", move |Data::<Watcher>(watcher)| {
			let to_socket = watcher_socketio.get_socket(Sid::from_str(&watcher.to_id).unwrap()).unwrap();
			to_socket.emit("watcher", watcher).unwrap();
		});
	});

	let handle = tokio::spawn(async move {
		let app = Router::new()
			.route("/", get(root))
			.nest_service("/static", ServeDir::new("static"))
			.layer(socketio_layer);

		let mut make_service = app.into_make_service_with_connect_info::<UdsConnectInfo>();

		loop {
			let (socket, _remote_addr) = uds.accept().await.unwrap();

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