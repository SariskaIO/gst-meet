#![feature(libc)]
extern crate libc;
extern crate strfmt;
use actix::Addr;
use futures::FutureExt;
use strfmt::strfmt;
use std::env::{set_var};
use std::env;
use std::f32::consts::E;
use actix_web::{get, web, post, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use jsonwebtoken::{decode ,decode_header,  Algorithm, DecodingKey, Validation};
use std::process::{Command, Stdio};
use std::time::{SystemTime};
use rand::distributions::{Alphanumeric, DistString};
use reqwest::header::{HeaderMap};
use redis::{Client, aio::MultiplexedConnection};
use actix::Message;
use std::panic;
use minreq;
use serde_json::Error;
use uuid::Uuid;
use std::io::{BufRead, BufReader};
use std::thread;
use nix::unistd::Pid;
use nix::sys::signal::{self, Signal};
use url::Url;
use serde_json::{json, Value};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

#[derive(Message, Debug)]
#[rtype(result = "Result<Option<String>, redis::RedisError>")]
pub struct InfoCommandGet {
    pub command: String,
    pub arg: String,
    pub arg2: Option<String>
}


#[derive(Message, Debug)]
#[rtype(result = "Result<Option<String>, redis::RedisError>")]
pub struct InfoCommandSet {
    pub command: String,
    pub arg: String,
    pub arg2: String
}

#[derive(Message, Debug)]
#[rtype(result = "Result<Option<String>, redis::RedisError>")]
pub struct InfoCommandDel {
    pub command: String,
    pub arg: String
}

#[derive(Message, Debug)]
#[rtype(result = "Result<Option<String>, redis::RedisError>")]
pub struct InfoCommandPublish {
    pub command: String,
    pub channel: String,
    pub message: String
}

#[derive(Clone)]
pub struct RedisActor {
    pub conn: MultiplexedConnection
}

use std::{collections::HashMap, sync::RwLock};
use libc::{kill, SIGTERM};

// This struct represents state
#[derive(Clone)]
pub struct AppState {
    pub map: HashMap<String,  String>,
    pub conn: Addr<RedisActor>,
    pub is_recording: Arc<AtomicBool>,
}


#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String
}


#[derive(Debug, Serialize, Deserialize)]
pub struct Context {
    pub group: String,
    pub user: User  
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub context: Context
}

#[derive(Serialize, Deserialize, Debug)]
struct PublicKey {
    e: String,
    n: String,
    kty: String
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Params {
    room_name: String,
    audio_only: Option<bool>,
    video_only: Option<bool>,
    is_vod: Option<bool>,
    profile: Option<String>,
    reconnect_window: Option<u64>,
    layout:  Option<String>,
    codec: Option<String>,
    multi_bitrate: Option<bool>,
    is_low_latency: Option<bool>,
    ingest_url: Option<String>,
    username: Option<bool>,
    uuid: Option<String>,
    is_recording: Option<bool>,
    stream_urls: Option<Vec<String>>,
    stream_keys: Option<Vec<StreamKeyDict>>
}

#[derive(Debug, Deserialize, Serialize)]
struct RtmpParams {
    room_name: String,
    audio_only: Option<bool>,
    video_only: Option<bool>,
    is_vod: Option<bool>,
    uuid: String,
    app_id: String,
    owner_id: String,
    user_id: String,
    pod_ip: String,
    origin_pod_ip: String,
    is_recording: Option<bool>,
    stream_urls: Option<Vec<String>>,
    stream_keys: Option<Vec<StreamKeyDict>>
}


#[derive(Debug, Deserialize, Serialize, Clone)]
struct StreamKeyDict {
    key: String,
    value: String,
}

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

#[derive(Serialize, Deserialize, Debug)]
struct InnerData {
    ip: String,
    port: u16
}

#[derive(Serialize, Deserialize, Debug)]
struct SchedulerData {
    data: Origin
}

#[derive(Serialize, Deserialize, Debug)]
struct Origin {
    origin: InnerData
}

#[derive(Serialize)]
struct ResponseVideoStart {
    started: bool,
    stream_name: String,
    pod_name: String,
    hls_url: Option<String>,
    hls_cdn_url: Option<String>,
    hls_master_url: Option<String>,
    hls_master_cdn_url: Option<String>,
    low_latency_hls_url: Option<String>,
    low_latency_hls_cdn_url: Option<String>,
    low_latency_hls_master_url: Option<String>,
    low_latency_hls_master_cdn_url: Option<String>,
    vod_url: Option<String>,
    rtmp_url: Option<String>,
    flv_url: Option<String>,
}

#[derive(Serialize)]
struct ResponseStop {
    started: bool
}


#[derive(Serialize)]
struct ResponseRecordingAlreadyStarted {
    started: bool,
    message: String,
}



#[derive(Serialize, Deserialize, Debug)]
pub struct SetRoomInfo {
    pub hostname: String,
    pub process_id: String,
    pub room_name: String,
}

pub async fn start_recording( 
        _req: HttpRequest,
        params: web::Json<Params>,
        app_state: web::Data<RwLock<AppState>>
    ) -> HttpResponse {

    let multi_bitrate = match params.multi_bitrate {
        Some(v) => v,
        _ => false,
    };

    {
        let mut state = app_state.write().unwrap();
        if state.is_recording.load(Ordering::SeqCst){
            return HttpResponse::NotFound().finish();
        }else {
            state.is_recording.store(true, Ordering::SeqCst);
        }
    }

    let mut app: String =  Alphanumeric.sample_string(&mut rand::thread_rng(), 16).to_lowercase();
    let stream: String =  Alphanumeric.sample_string(&mut rand::thread_rng(), 16).to_lowercase();
    let mut redis_actor = &app_state.read().unwrap().conn;
    let _auth = _req.headers().get("Authorization");

    let mut location;
    let gstreamer_pipeline;
    let _split: Vec<&str> = _auth.unwrap().to_str().unwrap().split("Bearer").collect();
    let token = _split[1].trim();

    println!("{}/{}", token, params.room_name);
    set_var("ROOM_NAME", &params.room_name.clone().to_string());
    set_var("AUTH_TOKEN", &token.clone().to_string());


    print!("{:?} params.audio_only ", params.audio_only );
    let my_uuid = Uuid::new_v4();
    let new_uuid = format!("{}", my_uuid.to_simple());

    let header  =  decode_header(&token);
    let request_url = env::var("SECRET_MANAGEMENT_SERVICE_PUBLIC_KEY_URL").unwrap_or("none".to_string());
    
    let header_data = match header {
        Ok(_token) => _token.kid,
        Err(_e) => None,
    };
    let kid = header_data.as_deref().unwrap_or("default string");
        // create a Sha256 object
    let api_key_url =  format!("{}/{}", request_url, kid);
    println!("{:?}", api_key_url);
    let decoded_claims;
    let claims;
    let response = minreq::get(api_key_url).send();
    match response {
            Ok(response)=>{
                let public_key = response.as_str().unwrap_or("default");
                let deserialized: PublicKey = serde_json::from_str(&public_key).unwrap();
                decoded_claims = decode::<Claims>(
                    &token,
                    &DecodingKey::from_rsa_components(&deserialized.n, &deserialized.e),
        &Validation::new(Algorithm::RS256));
                    match decoded_claims {
                        Ok(v) => {
                            claims = v;
                        },
                        Err(e) => {
                        println!("Error decoding json: {:?}", e);
                        return HttpResponse::Unauthorized().json("{}");
                        },
                    }
            },
            _=>{
                return HttpResponse::Unauthorized().json("{}");
            }
    }

    let response = minreq::get(env::var("ORIGIN_CLUSTER_SCHEDULER").unwrap_or("none".to_string())).send();
    let RTMP_OUT_LOCATION;
    match response {
        Ok(response)=>{
            let response_as_str = response.as_str().unwrap_or("{}");
            println!("{}", response_as_str);
            let deserialized: SchedulerData = serde_json::from_str(&response_as_str).unwrap();
            println!("{:?}", deserialized);
            RTMP_OUT_LOCATION = format!("rtmp://{}:{}", deserialized.data.origin.ip, deserialized.data.origin.port.to_string()); 
        },
        _=>{
            RTMP_OUT_LOCATION = "rtmp://srs-origin-0.socs:1935".to_owned() // fallback in case origin cluster scheduler is down
        }
    }

    let url = Url::parse(&RTMP_OUT_LOCATION).unwrap();
    let hostname = url.host_str().unwrap();
    println!("{}", hostname);
    let encoded = serde_json::to_string(&RtmpParams {
        audio_only: params.audio_only,
        video_only: params.video_only,
        is_vod: params.is_vod,
        user_id: claims.claims.context.user.id,
        owner_id: claims.claims.context.group,
        app_id: claims.claims.sub,
        origin_pod_ip: hostname.to_string(),
        uuid: new_uuid.to_lowercase(),
        room_name: params.room_name.clone(),
        is_recording: params.is_recording.clone(),
        pod_ip: env::var("MY_POD_NAME").unwrap_or("none".to_string()),
        stream_keys: params.stream_keys.clone(),
        stream_urls: params.stream_urls.clone()
    });
    
    let encoded = match encoded {
        Ok(v) => v,
        _ => "test".to_owned()
    };

    let codec = match  &params.codec {
        Some(v) => v,
        _ => "H264"
    };

    let layout = match &params.layout {
        Some(v) => v,
        _ => "desktop",
    };

    let ingest_url = match &params.ingest_url{
        Some(v) => v,
        _ => "",
    };

    let username = match params.username {
        Some(v) => v,
        _ => false
    };

    let resolution = match &params.profile {
        Some(v) => v,
        _ => "adaptive",
    };

    let is_low_latency = match params.is_low_latency {
        Some(v) => v,
        _ => false,
    };

    let audio_only = match params.audio_only {
        Some(v) => v,
        _ => false,
    };

    let is_vod = match params.is_vod {
        Some(v) => v,
        _ => false,
    };

    let video_only = match params.video_only {
        Some(v) => v,
        _ => false,
    };

    match params.reconnect_window {
        Some(value) => {
            set_var("RECONNECT_WINDOW", &value.to_string());
        },
        None => {
        // Handle the case where the value is None
        }
    };

    if layout == "mobile" {  
        set_var("LAYOUT", "mobile");
    }
    
    if username {  
        set_var("USERNAME", "true");
    }

    println!("Setting {} {} {}", layout, username, resolution);

    let api_host = env::var("API_HOST").unwrap_or("none".to_string());
    let xmpp_muc_domain = env::var("XMPP_MUC_DOMAIN").unwrap_or("none".to_string());
    let xmpp_domain = env::var("XMPP_DOMAIN").unwrap_or("none".to_string());

    let ingest_source = if !ingest_url.is_empty() {
        format!("uridecodebin uri={} name=dec \
             dec. ! queue ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! audio. \
             dec. ! queue ! videoscale ! video/x-raw,width=640,height=360 ! videoconvert ! video/x-raw,format=I420 ! queue ! video.sink_0 ", ingest_url)
    } else {
        String::new()
    };

    // Build location dynamically
    let (video_width, video_height, profile, vhost) = match (resolution, layout, is_low_latency, multi_bitrate) {
        ("HD", _, _, _) => (1280, 720, "HD", "transcode"),
        (_, "mobile", true, _) => (360, 640, "", if codec == "H265" { "ll_latency_h265" } else { "ll_latency_h264" }),
        (_, "mobile", false, _) => (360, 640, "", ""),
        (_, _, true, _) => (1280, 720, "", if codec == "H265" { "ll_latency_h265" } else { "ll_latency_h264" }),
        (_, _, true, true) => (1280, 720, "", if codec == "H265" { "ll_latency_multi_bitrate_h265" } else { "ll_latency_multi_bitrate_h264" }),
        (_, _, _, true) => (1280, 720, "HD", "transcode"),
        _ => (1280, 720, "", ""),  // Default (adaptive quality)
    };
    
;    // Shared pipeline components
    let shared_pipeline = format!(
        "/usr/local/bin/gst-meet \
        --web-socket-url=wss://{}/api/v1/media/websocket \
        --xmpp-domain={} \
        --muc-domain={} \
        --recv-video-scale-width={} \
        --recv-video-scale-height={} \
        --room-name={} \
        --recv-pipeline='audiomixer name=audio ! queue2 ! voaacenc bitrate=96000 ! mux.",
        api_host, xmpp_domain, xmpp_muc_domain, video_width, video_height, params.room_name
    );
    

    location = format!("{}/{}/{}", RTMP_OUT_LOCATION, app, stream);
    location = format!("{}?vhost={}&param={}", location, vhost, encoded);

    // Set PROFILE if needed
    if profile != "" {
        set_var("PROFILE", profile);
    }

    // Dynamically build the rest of the gstreamer pipeline
    gstreamer_pipeline = match (audio_only, video_only) {
        (true, false) => format!("{} audio/mpeg ! aacparse ! audio/mpeg, mpegversion=4 \
                                    ! flvmux streamable=true name=mux \
                                    ! rtmpsink location={}'", shared_pipeline, location),
        (false, true) => format!("{} \
                                    {} \
                                    compositor name=video background=black \
                                    ! x264enc \
                                    ! video/x-h264,profile=main \
                                    ! flvmux streamable=true name=mux \
                                    ! rtmpsink location={}'", shared_pipeline, ingest_source, location),
        _ => format!("{} \
                    {} \
                    compositor name=video background=black \
                    ! videoscale \
                    ! video/x-raw, width={}, height={}\
                    ! x264enc {} \
                    ! video/x-h264,profile={} \
                    ! flvmux streamable=true name=mux \
                    ! rtmpsink location={}'", shared_pipeline, ingest_source, video_width*2, video_height*2,if is_low_latency { "speed-preset=ultrafast tune=zerolatency" } else { "" }, if video_width == 360 { "main" } else { "high" }, location), // Conditional x264enc parameters and profile
    };

    println!("gstreamer-pipeline: {}", gstreamer_pipeline);

    let child = Command::new("sh")
    .arg("-c")
    .arg(gstreamer_pipeline)
    .spawn()
    .expect("failed to execute process");
    println!("Started process: {}", child.id());
    println!("{} print child process id", child.id().to_string());

    let hostname = env::var("MY_POD_NAME").unwrap_or("none".to_string());
    let room_info = SetRoomInfo {
        room_name: params.room_name.to_string(),
        process_id: child.id().to_string().clone(),
        hostname: hostname
    };

    thread::spawn(move || {
        let mut f = BufReader::new(child.stdout.unwrap());
        loop {
            let mut buf = String::new();
            match f.read_line(&mut buf) {
                Ok(_) => {
                    buf.as_str();
                }
                Err(e) => println!("an error!: {:?}", e),
            }
        }
    });
    let comm = InfoCommandSet {
        command: "SET".to_string(),
        arg2: serde_json::to_string(&room_info).unwrap(),
        arg: format!("production::room_key::{}", params.room_name).to_string()
    };
    redis_actor.send(comm).await;
    let obj = create_response_start_video(app.clone(), stream.clone(), new_uuid.clone(), is_low_latency.clone(), codec.clone().to_string(), is_vod.clone(), multi_bitrate.clone());
    HttpResponse::Ok().json(obj)
}

fn create_response_start_video(app: String, stream: String, uuid: String, is_low_latency: bool, codec: String, is_vod: bool, multi_bitrate: bool) -> serde_json::Value {
    let HLS_HOST = env::var("HLS_HOST").unwrap_or("none".to_string());
    let HLS_HOST_CDN = env::var("HLS_HOST_CDN").unwrap_or("none".to_string()); // new cdn host for normal hls
    let LOW_LATENCY_HLS_HOST = env::var("LOW_LATENCY_HLS_HOST").unwrap_or("none".to_string());
    let LOW_LATENCY_HLS_HOST_CDN = env::var("LOW_LATENCY_HLS_HOST_CDN").unwrap_or("none".to_string()); // new cdn host for low latency
    let VOD_HOST = env::var("VOD_HOST").unwrap_or("none".to_string());
    let EDGE_UDP_PLAY = env::var("EDGE_UDP_PLAY").unwrap_or("none".to_string());
    let EDGE_TCP_PLAY = env::var("EDGE_TCP_PLAY").unwrap_or("none".to_string());

     let mut ll_latency_host = match codec.as_str() {
    "H264" => "ll_latency_h264",
    "H265" => "ll_latency_h265",
    _ => LOW_LATENCY_HLS_HOST.as_str(),
    };

if multi_bitrate && is_low_latency {
    if codec == "H264" {
        ll_latency_host = "ll_latency_multi_bitrate_h264";
    } else if codec == "H265" {
        ll_latency_host = "ll_latency_multi_bitrate_h265";
    }
}
   

       let mut obj = json!({
        "started": true,
        "stream_name": app.clone(),
        "pod_name": env::var("MY_POD_NAME").unwrap_or("none".to_string()),
        "hls_url": None::<Value>,
        "hls_cdn_url": None::<Value>,
        "hls_master_url": None::<Value>,
        "hls_master_cdn_url": None::<Value>,
        "low_latency_hls_url": None::<Value>,
        "low_latency_hls_url_cdn": None::<Value>,
        "low_latency_hls_master_url": None::<Value>,
        "low_latency_hls_master_url_cdn": None::<Value>,
        "vod_url": None::<Value>,
        "rtmp_url": None::<Value>,
        "flv_url": None::<Value>,
    });
 
    if is_vod {
        obj["vod_url"] = json!(format!("https://{}/{}/index.m3u8", VOD_HOST, uuid));
    }
    
    if is_low_latency && multi_bitrate {
        obj["low_latency_hls_master_url"] = json!(format!("https://{}/multi/{}/{}/master.m3u8", LOW_LATENCY_HLS_HOST, app, stream));
        obj["low_latency_hls_master_cdn_url"] = json!(format!("https://{}/multi/{}/{}/master.m3u8", LOW_LATENCY_HLS_HOST_CDN, app, stream));
    } else if is_low_latency {
        obj["low_latency_hls_url"] = json!(format!("https://{}/original/{}/{}/playlist.m3u8", LOW_LATENCY_HLS_HOST, app, stream));
        obj["low_latency_hls_cdn_url"] = json!(format!("https://{}/original/{}/{}/playlist.m3u8", LOW_LATENCY_HLS_HOST_CDN, app, stream));
    } else if multi_bitrate {
        obj["hls_master_url"] = json!(format!("https://{}/play/hls/{}/{}/master.m3u8", HLS_HOST, app, stream));
        obj["hls_master_cdn_url"] = json!(format!("https://{}/play/hls/{}/{}/master.m3u8", HLS_HOST_CDN, app, stream));
    } else {
        obj["hls_url"] = json!(format!("https://{}/play/hls/{}/{}.m3u8", HLS_HOST, app, stream));
        obj["hls_cdn_url"] = json!(format!("https://{}/play/hls/{}/{}.m3u8", HLS_HOST_CDN, app, stream));
    } 
    
    if is_low_latency && multi_bitrate {
        obj["rtmp_url"] = json!(format!("rtmp://{}:1935/{}/{}?vhost={}", EDGE_TCP_PLAY, app, stream, ll_latency_host));
        obj["flv_url"] = json!(format!("http://{}:8936/{}/{}.flv?vhost={}", EDGE_TCP_PLAY, app, stream, ll_latency_host));
    } else if is_low_latency {
        obj["rtmp_url"] = json!(format!("rtmp://{}:1935/{}/{}?vhost={}", EDGE_TCP_PLAY, app, stream, ll_latency_host));
        obj["flv_url"] = json!(format!("http://{}:8936/{}/{}.flv?vhost={}", EDGE_TCP_PLAY, app, stream, ll_latency_host));
    } else if multi_bitrate {
        obj["rtmp_url"] = json!(format!("rtmp://{}:1935/{}/{}?vhost={}", EDGE_TCP_PLAY, app, stream, "transcode",));
        obj["flv_url"] = json!(format!("http://{}:8936/{}/{}.flv?vhost={}", EDGE_TCP_PLAY, app, stream, "transcode",));
    } else {
        obj["rtmp_url"] = json!(format!("rtmp://{}:1935/{}/{}", EDGE_TCP_PLAY, app, stream));
        obj["flv_url"] = json!(format!("http://{}:8936/{}/{}.flv", EDGE_TCP_PLAY, app, stream));
    }

    obj.as_object().map(|map| {
        map.iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<serde_json::Map<_, _>>()
    }).map(|filtered_map| {
        json!(filtered_map)
    }).unwrap_or(json!(null))
}

pub async fn stop_recording( 
        _req: HttpRequest,
        params: web::Json<Params>,
        app_state: web::Data<RwLock<AppState>>
    ) -> HttpResponse {

    println!("{:?}", params);
    let _auth = _req.headers().get("Authorization");
    let _split: Vec<&str> = _auth.unwrap().to_str().unwrap().split("Bearer").collect();
    let token = _split[1].trim();
    let mut redis_actor = &app_state.read().unwrap().conn;

    let comm = InfoCommandGet {
        command: "GET".to_string(),
        arg: format!("production::room_key::{}", params.room_name).to_string(),
        arg2: None,
    };
    
    let mut run_async = || async move {
        redis_actor.send(comm).await
    };

    let result = async move {
        // AssertUnwindSafe moved to the future
        std::panic::AssertUnwindSafe(run_async()).catch_unwind().await
    }.await;        

    match result {
        Ok(Ok(Ok(Some(value))))  => {
           let room_info: SetRoomInfo = serde_json::from_str(&value).unwrap();
           let hostname = env::var("MY_POD_NAME").unwrap_or("none".to_string());
           println!("{:?}", room_info);
           if room_info.hostname != "" {
               if hostname == room_info.hostname {
                    let my_int = room_info.process_id.parse::<i32>().unwrap();
                    unsafe {
                        signal::kill(Pid::from_raw(my_int), Signal::SIGTERM).unwrap();
                    }
               } else {
                    let comm = InfoCommandPublish {
                        command: "PUBLISH".to_string(),
                        channel: "sariska_channel_gstreamer".to_string(),
                        message: value
                    };
                    redis_actor.send(comm).await;
               }
            }
        },
        Ok(Ok(Ok(None))) => (),
        Err(_)=> (),
        Ok(Err(_))=>(),
        Ok(Ok(Err(_)))=>()
    };

    let comm = InfoCommandDel {
        command: "DEL".to_string(),
        arg: format!("production::room_key::{}", params.room_name).to_string(),
    };
    
    let mut run_async = || async move {
        redis_actor.send(comm).await
    };

    let result = async move {
        // AssertUnwindSafe moved to the future
        std::panic::AssertUnwindSafe(run_async()).catch_unwind().await
    }.await;
    
    let obj = ResponseStop {
        started: false
    };
    HttpResponse::Ok().json(obj)
}

