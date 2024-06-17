use actix_web::{get, web, App, HttpServer, Responder, HttpRequest, HttpResponse};
mod repositories;
pub use repositories::AppState;
use std::env;
use std::thread;
use std::{collections::HashMap, pin::Pin, sync::RwLock};
use actix::prelude::*;
use actix::Message;
use libc::{kill, SIGTERM};
use serde_json::Error;
use nix::unistd::Pid;
use nix::sys::signal::{self, Signal};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use actix_rt::signal::unix::signal;
use tokio::sync::broadcast;
use tokio::signal::unix::SignalKind;
#[cfg(unix)]
use tokio::signal::unix;
use actix_rt::signal::unix::SignalKind as ActixSignalKind;
use futures::StreamExt;
use serde_json::de::Read;
use std::iter::Iterator;
use actix_web_prometheus::{PrometheusMetrics, PrometheusMetricsBuilder};
   
pub async fn get_health_status() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body("Healthy!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let is_recording = Arc::new(AtomicBool::new(false));

    let prometheus = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    
    HttpServer::new(move || {
        App::new()
            .wrap(prometheus.clone())
            .app_data( 
                web::Data::new(RwLock::new(AppState {
                    map: HashMap::new(),
                    is_recording: is_recording.clone()
            })).clone())
            .service(web::resource("/user/startRecording").route(web::post().to(repositories::user_repository::start_recording)))
            .service(web::resource("/user/stopRecording").route(web::post().to(repositories::user_repository::stop_recording)))
            .service(web::resource("/healthz").route(web::get().to(get_health_status)))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
