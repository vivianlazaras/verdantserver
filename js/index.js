import { connect, RoomEvent, createLocalVideoTrack } from 'livekit-client';

// /home/cardinal/projects/security/physical/verdanthaven/js/index.js
// Requires: npm install livekit-client

/**
 * Setup a LiveKit client, join a room and render video tracks into a given container.
 * @param {string} token - LiveKit join token.
 * @param {string} roomName - Room name (not strictly required by connect if token already grants access).
 * @param {HTMLElement} videoContainer - DOM element where remote/local video elements will be appended.
 * @returns {Promise<{ room: import('livekit-client').Room, disconnect: ()=>Promise<void>, publishLocalVideo: ()=>Promise<void> }>}
 */
export async function setupLiveKit(token, roomName, videoContainer) {
    if (!token || typeof token !== 'string') throw new Error('token (string) required');
    if (!roomName || typeof roomName !== 'string') console.warn('roomName is empty; token may already define room access');
    if (!(videoContainer instanceof HTMLElement)) throw new Error('videoContainer must be a DOM element');

    // Default server URL: same origin (wss for https, ws for http).
    // Override by setting window.LIVEKIT_URL if needed.
    const defaultUrl =
        (location.protocol === 'https:' ? 'wss' : 'ws') +
        '://' +
        location.hostname +
        (location.port ? `:${location.port}` : '');
    const url = window.LIVEKIT_URL || defaultUrl;

    const room = await connect(url, token, {
        // tweak options as needed
        autoSubscribe: true,
        adaptiveStream: true,
    });

    // Helper to attach a track (local or remote) into the container.
    const attachTrack = (track) => {
        const el = track.attach();
        el.style.maxWidth = '100%';
        el.style.maxHeight = '100%';
        videoContainer.appendChild(el);
        return el;
    };

    // Helper to detach and remove any attached elements for a track.
    const detachTrack = (track) => {
        const els = track.detach();
        els.forEach((el) => {
            if (el.parentElement) el.parentElement.removeChild(el);
        });
    };

    // Handle tracks as they are subscribed/unsubscribed.
    room.on(RoomEvent.TrackSubscribed, (track, publication, participant) => {
        if (track.kind === 'video') {
            attachTrack(track);
        }
    });

    room.on(RoomEvent.TrackUnsubscribed, (track, publication, participant) => {
        if (track && track.kind === 'video') {
            detachTrack(track);
        }
    });

    // If participants already have subscribed tracks when joining, attach them.
    for (const participant of room.participants.values()) {
        for (const pub of participant.tracks.values()) {
            const track = pub.track;
            if (track && track.kind === 'video' && pub.isSubscribed) {
                attachTrack(track);
            }
        }
    }

    // Optionally attach local video track preview when published.
    async function publishLocalVideo() {
        const localTrack = await createLocalVideoTrack({ facingMode: 'user' });
        await room.localParticipant.publishTrack(localTrack);
        attachTrack(localTrack);
    }

    async function disconnect() {
        try {
            await room.disconnect();
        } catch (e) {
            console.warn('error disconnecting from livekit', e);
        }
    }

    // return useful handles
    return {
        room,
        disconnect,
        publishLocalVideo,
    };
}