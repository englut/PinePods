use crate::components::context::UIState;
use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{MediaMetadata, MediaSession, MediaSessionPlaybackState};
use yewdux::prelude::Dispatch;

pub const SKIP_FORWARD_SECS: f64 = 30.0;
pub const SKIP_BACK_SECS: f64 = 15.0;

fn media_session() -> Option<MediaSession> {
    let navigator = web_sys::window()?.navigator();
    let session = Reflect::get(&navigator, &JsValue::from_str("mediaSession")).ok()?;
    if session.is_undefined() || session.is_null() {
        return None;
    }
    Some(navigator.media_session())
}

/// Where a relative seek of `delta` seconds lands, kept inside the episode.
pub fn seek_target(current: f64, delta: f64, duration: f64) -> f64 {
    let target = (current + delta).max(0.0);
    if duration.is_finite() && duration > 0.0 {
        target.min(duration)
    } else {
        target
    }
}

fn seek_by(delta: f64) {
    let dispatch = Dispatch::<UIState>::global();
    let state = dispatch.get();
    let (current, duration) = if let Some(media) = &state.media_element {
        (media.current_time(), media.duration())
    } else if let Some(audio) = &state.audio_element {
        (audio.current_time(), audio.duration())
    } else {
        return;
    };
    let target = seek_target(current, delta, duration);
    if let Some(media) = &state.media_element {
        media.set_current_time(target);
    } else if let Some(audio) = &state.audio_element {
        audio.set_current_time(target);
    }
    dispatch.reduce_mut(|s| s.update_current_time(target));
}

fn set_playing(playing: bool) {
    Dispatch::<UIState>::global().reduce_mut(move |s| {
        if s.audio_playing.unwrap_or(false) != playing {
            s.toggle_playback();
        }
    });
}

fn set_handler(session: &MediaSession, action: &str, handler: impl FnMut() + 'static) {
    let Ok(set) = Reflect::get(session, &JsValue::from_str("setActionHandler")) else { return };
    let Ok(set) = set.dyn_into::<Function>() else { return };
    let closure = Closure::<dyn FnMut()>::new(handler);
    // Browsers throw for actions they don't support; the rest still get their handlers.
    let _ = set.call2(session, &JsValue::from_str(action), closure.as_ref());
    closure.forget();
}

/// Route the browser's media controls (keyboard media keys, headset buttons, the OS media
/// overlay) to the player. Next/previous track skip forward and back within the episode.
pub fn install_handlers() {
    let Some(session) = media_session() else { return };
    set_handler(&session, "play", || set_playing(true));
    set_handler(&session, "pause", || set_playing(false));
    set_handler(&session, "seekforward", || seek_by(SKIP_FORWARD_SECS));
    set_handler(&session, "seekbackward", || seek_by(-SKIP_BACK_SECS));
    set_handler(&session, "nexttrack", || seek_by(SKIP_FORWARD_SECS));
    set_handler(&session, "previoustrack", || seek_by(-SKIP_BACK_SECS));
}

/// Show the current episode in the OS media controls and keep their play/pause state in step.
pub fn sync(title: Option<&str>, podcast: &str, artwork_url: &str, playing: bool) {
    let Some(session) = media_session() else { return };
    let Some(title) = title else {
        session.set_metadata(None);
        session.set_playback_state(MediaSessionPlaybackState::None);
        return;
    };
    if let Ok(metadata) = MediaMetadata::new() {
        metadata.set_title(title);
        metadata.set_artist(podcast);
        if !artwork_url.is_empty() {
            let art = Object::new();
            let _ = Reflect::set(&art, &"src".into(), &artwork_url.into());
            let _ = Reflect::set(&metadata, &"artwork".into(), &Array::of1(&art));
        }
        session.set_metadata(Some(&metadata));
    }
    session.set_playback_state(if playing {
        MediaSessionPlaybackState::Playing
    } else {
        MediaSessionPlaybackState::Paused
    });
}

#[cfg(test)]
mod tests {
    use super::seek_target;

    #[test]
    fn seeks_relative_to_the_current_position() {
        assert_eq!(seek_target(100.0, 30.0, 600.0), 130.0);
        assert_eq!(seek_target(100.0, -15.0, 600.0), 85.0);
    }

    #[test]
    fn stays_inside_the_episode() {
        assert_eq!(seek_target(5.0, -15.0, 600.0), 0.0);
        assert_eq!(seek_target(590.0, 30.0, 600.0), 600.0);
    }

    #[test]
    fn an_unknown_duration_only_clamps_at_the_start() {
        assert_eq!(seek_target(590.0, 30.0, f64::NAN), 620.0);
        assert_eq!(seek_target(590.0, 30.0, 0.0), 620.0);
    }
}
