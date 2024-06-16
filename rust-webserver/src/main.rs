use actix_web::{get, web, App, HttpServer, Responder, HttpRequest, HttpResponse};
mod repositories;
use redis::AsyncCommands;
use redis::ControlFlow;
use redis::PubSubCommands;
pub use repositories::AppState;
pub use repositories::SetRoomInfo;
pub use repositories::RedisDatabase;
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
use redis::cluster::ClusterClient;
use redis::cluster::ClusterConnection;
use redis::cluster::ClusterClientBuilder;
use redis::{Client, RedisError};
use redis::{aio::MultiplexedConnection};
use redis::aio::PubSub;
use redis::aio::PubSub as OtherPubSub;
use actix_rt::signal::unix::signal;
use tokio::sync::broadcast;
use redis::aio::{Connection, ConnectionLike};
use tokio::signal::unix::SignalKind;
#[cfg(unix)]
use tokio::signal::unix;
use actix_rt::signal::unix::SignalKind as ActixSignalKind;
use futures::StreamExt;
use redis::Commands;
use serde_json::de::Read;
   
use std::iter::Iterator;
   
impl RedisDatabase {
    async fn new() -> Result<Self, RedisError> {
        let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
        // Create a standalone Redis connection for Pub/Sub
        println!("redis_url.......{}", redis_url);
        // let client = Client::open("redis://127.0.0.1/")?;
        let client = Client::open(redis_url.clone()).unwrap();
        println!("client..{?}",client); 
        // thread::spawn(move || {
        //     let mut con = client.get_connection().unwrap();
        //     let _ :() =  con.subscribe(&["sariska_channel_gstreamer"], |msg| {
        //         let ch = msg.get_channel_name();
        //         let payload: String = msg.get_payload().unwrap();
        //         let decoded: SetRoomInfo  = serde_json::from_str(&payload).unwrap();
        //         let hostname = env::var("MY_POD_NAME").unwrap_or("none".to_string());

        //         println!("{} hostname", hostname);
        //         if decoded.hostname != "" {
        //             println!("{:?} subscribed", decoded);
        //             if  hostname == decoded.hostname {
        //                 let my_int = decoded.process_id.parse::<i32>().unwrap();
        //                 unsafe {
        //                     println!(" killed process id {} ", my_int);
        //                     signal::kill(Pid::from_raw(my_int), Signal::SIGTERM).unwrap();
        //                 }
        //             }
        //         }
        //         return ControlFlow::Continue;
        //     }).unwrap();
        // });

        let client_clone = client.clone();

        thread::spawn(move || {
            match client_clone.get_connection() {
                Ok(mut con) => {
                    let _ :() = con.subscribe(&["sariska_channel_gstreamer"], |msg| {
                        let ch = msg.get_channel_name();
                        let payload: String = msg.get_payload().unwrap();
                        let decoded: SetRoomInfo = serde_json::from_str(&payload).unwrap();
                        let hostname = env::var("MY_POD_NAME").unwrap_or("none".to_string());

                        println!("{} hostname", hostname);
                        if !decoded.hostname.is_empty() {
                            println!("{:?} subscribed", decoded);
                            if hostname == decoded.hostname {
                                if let Ok(my_int) = decoded.process_id.parse::<i32>() {
                                    unsafe {
                                        println!("Killed process id {}", my_int);
                                        signal::kill(Pid::from_raw(my_int), Signal::SIGTERM).unwrap();
                                    }
                                }
                            }
                        }
                        ControlFlow::Continue
                    }).unwrap();
                },
                Err(e) => {
                    eprintln!("Failed to establish Redis connection for Pub/Sub: {:?}", e);
                }
            }
        });

        let redis_password = env::var("REDIS_PASSWORD").ok();
    
        let mut builder = ClusterClientBuilder::new(vec![redis_url]);
        if let Some(password) = redis_password {
            builder = builder.password(password);
        }
    
        let client = builder.open()?;
        let connection = client.get_connection()?;
        let cluster_client = Arc::new(Mutex::new(connection));


        Ok(RedisDatabase { cluster_client })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, RedisError> {
        let mut con = self.cluster_client.lock().unwrap();
        con.get(key)
    }
    
    async fn del(&self, key: &str) -> Result<(), RedisError> {
        let mut con = self.cluster_client.lock().unwrap();
        con.del(key)?;
        Ok(())
    }

    async fn set(&self, key: &str, value: &[u8], expiry: usize) -> Result<(), RedisError> {
        let mut con = self.cluster_client.lock().unwrap();
        con.set(key, value)?;
        con.expire(key, expiry)?;
        Ok(())
    }

    async fn publish(&self, message: &str) -> Result<(), RedisError> {
        let mut con = self.cluster_client.lock().unwrap();
        con.publish("sariska_channel_gstreamer", message)?;
        Ok(())
    }
}

pub async fn get_health_status() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body("Healthy!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let database = match RedisDatabase::new().await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            eprintln!("Failed to initialize RedisDatabase: {:?}", e);
            std::process::exit(1);
        }
    };
    let is_recording = Arc::new(AtomicBool::new(false));
    
    HttpServer::new(move || {
        App::new()
            .app_data( 
                web::Data::new(RwLock::new(AppState {
                    map: HashMap::new(),
                    conn: database.clone(),
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
