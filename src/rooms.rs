use crate::errors::VerdantErr;
use crate::utils::unix_now;
use chrono::{Local, TimeZone};
use futures::StreamExt;
use hound::WavWriter;
use livekit::participant::RemoteParticipant;
use livekit::track::{RemoteAudioTrack, RemoteTrack, RemoteVideoTrack};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit::{Room, RoomEvent, RoomOptions};
use mediawire::encoders::rav1ivf::Rav1eIvfPipeline;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use crate::rpc::LiveKitServer;
use tokio::sync::mpsc::UnboundedReceiver;

use livekit_api::access_token;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use mediawire::formats::PlanarBufferRef;

pub struct RoomState {
    name: String,
    /// is the room active (does it have any participants).
    active: bool,
}

impl RoomState {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            active: true,
        }
    }
}

async fn start_room_instance(
    state: Arc<Mutex<RoomState>>,
    url: &str,
    access_token: &str,
    session: ServerSession,
) -> Result<(), VerdantErr> {
    println!("in start room instance");
    let instance = RoomInstance::new(url, access_token).await?;
    instance.start_handler(session).await?;
    Ok(())
}

/// a struct used to indicate that a rooms recording session is active.
/// it also provides a means for the thread to check for whether or not it should be running.
pub struct RoomManager {
    rooms: Arc<Mutex<HashMap<String, Arc<Mutex<RoomState>>>>>,
    session: ServerSession,
}

impl RoomManager {
    pub fn new(session: ServerSession) -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
            session,
        }
    }

    pub async fn spawn_room(&self, name: &str, url: &str, cfg: &LiveKitServer) {
        println!("inside spawn room");
        let mut lock = self.rooms.lock().await;
        if let Some(room) = lock.get(name) {
            // room already exists, nothing to do.
        } else {
            let state = Arc::new(Mutex::new(RoomState::new(name)));
            lock.insert(name.to_string(), state.clone());
            // lock should be dropped here, otherwise a deadlock could happen.
            drop(lock);
            let verdant_token =
                access_token::AccessToken::with_api_key(&cfg.api_key, &cfg.api_secret)
                    .with_identity("verdant")
                    .with_name("verdant")
                    .with_grants(access_token::VideoGrants {
                        room_join: true,
                        room_record: true,
                        recorder: true,
                        can_subscribe: true,
                        room: "welcome".to_string(),
                        ..Default::default()
                    })
                    .to_jwt().expect("failed to create verdant access token");
            start_room_instance(state, url, &verdant_token, self.session.clone())
                .await
                .expect("failed to spawn room instance");
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerSession {
    start: i64,
    end: i64,
    session_dir: PathBuf,
}

impl ServerSession {
    pub fn from_root<P: AsRef<Path>>(root: P) -> Self {
        let start = unix_now();
        let end = 0;
        let session_dir = root.as_ref().join(epoch_to_local(start));
        Self {
            start,
            end,
            session_dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoomStore {
    server_session: ServerSession,
    name: String,
}

/// Convert a UNIX epoch (seconds since 1970-01-01 UTC)
/// into the local system's date/time.
///
/// # Arguments
/// * `epoch_secs` — UNIX timestamp in **seconds**
///
/// # Returns
/// * `DateTime<Local>` — the timestamp converted to the system's local timezone
pub fn epoch_to_local(epoch_secs: i64) -> String {
    let timestamp = Local
        .timestamp_opt(epoch_secs, 0)
        .single()
        .expect("invalid UNIX timestamp");
    format!("{}", timestamp)
}

impl RoomStore {
    pub fn new(session: ServerSession, name: String) -> Self {
        Self {
            server_session: session,
            name,
        }
    }
    pub fn get_participant_dir(&self, participant: &str) -> Result<PathBuf, std::io::Error> {
        let root = &self.server_session.session_dir;
        let participant_dir = root.join(&self.name).join(&participant);
        std::fs::create_dir_all(&participant_dir)?;
        Ok(participant_dir)
    }

    pub fn get_audio_dir(&self, participant: &str) -> Result<PathBuf, std::io::Error> {
        let dir = self.get_participant_dir(participant)?.join("audio");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    pub fn get_video_dir(&self, participant: &str) -> Result<PathBuf, std::io::Error> {
        let dir = self.get_participant_dir(participant)?.join("videos/");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// represents a means of recording video and audio.
pub struct RoomInstance {
    url: String,
    token: String,
    room: Arc<Room>,
    rx: UnboundedReceiver<RoomEvent>,
}

async fn record_audio(
    store: &RoomStore,
    track: RemoteAudioTrack,
    participant: RemoteParticipant,
) -> Result<(), VerdantErr> {

    let now = format!("{}", epoch_to_local(crate::utils::unix_now()));
    let root = store.get_audio_dir(&participant.identity().as_str())?;
    std::fs::create_dir_all(&root)?;
    let filename = root.join(&format!("{}.wav", now));
    println!("about to start recording to path: {}", filename.display());

    let channels = 2;
    let sample_rate = 48000;

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut audio_stream =
        NativeAudioStream::new(track.rtc_track(), sample_rate as i32, channels as i32);

    let mut writer = hound::WavWriter::create(&filename, spec)?;

    tokio::spawn(async move {
        let mut frame_count = 0;
        while let Some(frame) = audio_stream.next().await {
            frame_count += 1;
            // LiveKit gives audio as f32 PCM
            let samples = frame.data;

            for s in samples.iter() {
                let pcm = (*s * i16::MAX) as i16;
                if writer.write_sample(pcm).is_err() {
                    eprintln!("failed writing PCM sample");
                    break;
                }
            }
        }
        let _ = writer.finalize();
        eprintln!("Audio recording finished for {}", filename.display());
    });
    Ok(())
}

async fn record_video(
    store: &RoomStore,
    track: RemoteVideoTrack,
    participant: RemoteParticipant,
) -> Result<(), VerdantErr> {
    let timestamp = epoch_to_local(crate::utils::unix_now());
    let filename = store
        .get_video_dir(&participant.identity().to_string()).expect("failed to create video dir")
        .join(format!("{}.ivf", timestamp)).display().to_string();
    //std::fs::create_dir_all(&filename).expect("failed to create_dir_all for video filename");
    
    tokio::spawn(async move {
        let mut stream = NativeVideoStream::new(track.rtc_track());

        let first = if let Some(first) = stream.next().await {
            first
        } else {
            eprintln!("unable to get first frame from video track");
            return;
        };
        let second = if let Some(second) = stream.next().await {
            second
        } else {
            eprintln!("unable to get required second frame from video track");
            return;
        };
        let frame1: PlanarBufferRef = first.buffer.as_i420().unwrap().into();
        let frame2: PlanarBufferRef = second.buffer.as_i420().unwrap().into();

        let mut pipeline = Rav1eIvfPipeline::from_frames(
            &filename,
            &frame1,
            &frame2,
            first.timestamp_us,
            second.timestamp_us,
            10,
        );

        while let Some(frame) = stream.next().await {
            let buffer: PlanarBufferRef = frame.buffer.as_i420().unwrap().into();
            pipeline
                .push_frame(&buffer)
                .expect("failed to push frame to pipeline");
        }
        pipeline.finalize().unwrap();
    });
    Ok(())
}

impl RoomInstance {
    pub async fn new(
        url: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Self, crate::errors::VerdantErr> {
        let url = url.into();
        let token = token.into();
        let (room, rx) = Room::connect(&url, &token, RoomOptions::default()).await?;
        Ok(Self {
            url,
            token,
            room: Arc::new(room),
            rx,
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn token(&self) -> &str {
        &self.token
    }
    pub async fn start_handler(mut self, session: ServerSession) -> Result<(), VerdantErr> {
        println!("in start handler");
        let store = RoomStore::new(session, self.room.name());
        tokio::spawn(async move {
            while let Some(event) = self.rx.recv().await {
                match event {
                    RoomEvent::ParticipantConnected(rp) => {}
                    RoomEvent::ParticipantDisconnected(rp) => {}

                    RoomEvent::TrackSubscribed {
                        track,
                        publication,
                        participant,
                    } => {
                        println!("subscribed to new track with publication: {:?}", publication);
                        // now actually start recording.
                        match track {
                            RemoteTrack::Audio(rt) => { record_audio(&store, rt, participant).await; },
                            RemoteTrack::Video(rt) => { record_video(&store, rt, participant).await; },
                        }
                    }
                    RoomEvent::TrackPublished { publication, participant } => {
                        println!("TrackPublished: participant={} publication={:#?}", participant.identity(), publication);
                        publication.set_subscribed(true);
                    },
                    _ => {
                        //println!("event: {:?}", event);
                    }
                }
            }
            println!("after loop exiting the thread");
        });
        Ok(())
    }
}
