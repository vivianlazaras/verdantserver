use crate::errors::VerdantErr;
use tokio::runtime::Handle;
use std::path::PathBuf;
use livekit::track::{RemoteTrack, RemoteVideoTrack, RemoteAudioTrack};
use livekit::{RoomEvent, Room, RoomOptions};
use livekit::participant::RemoteParticipant;
use tokio::sync::mpsc::UnboundedReceiver;
use std::sync::Arc;
use chrono::{DateTime, Local, TimeZone};
use hound::WavWriter;
use futures::StreamExt;

use livekit::webrtc::audio_stream::native::NativeAudioStream;

#[derive(Debug, Clone)]
pub struct ServerSession {
    start: i64,
    end: i64,
    session_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RoomStore {
    server_session: ServerSession,
    name: String
}

pub struct ParticipantSession {
    /// identity of tbe participant.
    participant: String,
    connected: i64,
    /// if this is set to 0 then they haven't disconnected yet.
    disconnected: i64,
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
    let timestamp = Local.timestamp_opt(epoch_secs, 0)
        .single()
        .expect("invalid UNIX timestamp");
    format!("{}", timestamp)
}

impl RoomStore {
    pub fn new(session: ServerSession, name: String) -> Self {
        Self {
            server_session: session,
            name
        }
    }
    pub fn get_participant_dir(&self, participant: &str) -> PathBuf {
        let session_dir = epoch_to_local(self.server_session.start);
        let root = self.server_session.session_dir.join(&session_dir);
        let participant_dir = root.join(&self.name).join(&participant);
        participant_dir
    }
    
    pub fn get_audio_dir(&self, participant: &str) -> PathBuf {
        self.get_participant_dir(participant).join("/audio/")   
    }
    pub fn get_video_dir(&self, participant: &str) -> PathBuf {
        self.get_participant_dir(participant).join("/video/")
    }
}

/// represents a means of recording video and audio.
pub struct RoomManager {
    url: String,
    token: String,
    room: Arc<Room>,
    rx: UnboundedReceiver<RoomEvent>,
}

async fn record_audio(handle: Handle, store: &RoomStore, track: RemoteAudioTrack, participant: RemoteParticipant) -> Result<(), VerdantErr> {
    let now = format!("{}.wav", epoch_to_local(crate::utils::unix_now()));
    let root = store.get_audio_dir(&participant.identity().as_str());
    let filename = root.join(&now);
    println!("about to start recording to path: {}", filename.display());

    let channels = 2;
    let sample_rate = 48000;

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut audio_stream = NativeAudioStream::new(track.rtc_track(), sample_rate as i32, channels as i32);

    let mut writer = hound::WavWriter::create(&filename, spec)?;
    
    Ok(handle.spawn(async move {
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

            if frame_count > 1000 {
                writer.finalize().expect("finalization of wav file failed");
                let newnow = format!("{}.wav", epoch_to_local(crate::utils::unix_now()));
                let filename = root.join(&newnow);
;                writer = WavWriter::create(filename, spec.clone()).unwrap();
            }
        }
        let _ = writer.finalize();
        eprintln!("Audio recording finished for {}", filename.display());
    }).await?)
}

async fn record_video(handle: Handle, store: &RoomStore, track: RemoteVideoTrack, participant: RemoteParticipant) -> Result<(), VerdantErr> {
    Ok(handle.spawn(async move {

    }).await?)
}

impl RoomManager {
    pub async fn new(url: impl Into<String>, token: impl Into<String>) -> Result<Self, crate::errors::VerdantErr> {
        let url = url.into();
        let token = token.into();
        let (room, rx) = Room::connect(&url, &token, RoomOptions::default()).await?;
        Ok(Self {
            url,
            token,
            room: Arc::new(room),
            rx
        })
    }

    pub async fn start_handler(mut self, handle: Handle, session: ServerSession) -> Result<(), VerdantErr> {
        let store = RoomStore::new(session, self.room.name());
        handle.clone().spawn(async move {
            while let Some(event) = self.rx.recv().await {
                match event {
                    RoomEvent::ParticipantConnected(rp) => {},
                    RoomEvent::ParticipantDisconnected(rp) => {},

                    RoomEvent::TrackSubscribed { track, publication, participant } => {
                        // now actually start recording.
                        match track {
                            RemoteTrack::Audio(rt) => record_audio(handle.clone(), &store, rt, participant).await.expect("failed to start audio recorder"),
                            RemoteTrack::Video(rt) => record_video(handle.clone(), &store, rt, participant).await.expect("failed to start video recorder"), 
                        }
                    },
                    _ => unimplemented!(),
                }
            }
        }).await?;
        Ok(())
    }
}