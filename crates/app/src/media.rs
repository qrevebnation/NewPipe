#[cfg(not(target_os = "android"))]
mod imp {
    use nanopipe_core::StreamInfo;

    pub fn play(_info: StreamInfo) -> Result<(), String> {
        Ok(())
    }

    pub fn pause() -> Result<(), String> {
        Ok(())
    }

    pub fn resume() -> Result<(), String> {
        Ok(())
    }

    pub fn stop() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(target_os = "android")]
mod imp {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;
    use nanopipe_core::StreamInfo;

    fn with_env<F>(f: F) -> Result<(), String>
    where
        F: FnOnce(&mut jni::JNIEnv) -> Result<(), String>,
    {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        f(&mut env)
    }

    fn call_static(env: &mut jni::JNIEnv, name: &str, sig: &str) -> Result<(), String> {
        let class = env
            .find_class("dev/dioxus/main/PlaybackService")
            .map_err(|e| e.to_string())?;
        env.call_static_method(class, name, sig, &[])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn play(info: StreamInfo) -> Result<(), String> {
        with_env(|env| {
            let ctx = ndk_context::android_context();
            let context = unsafe { JObject::from_raw(ctx.context().cast()) };
            let url = env.new_string(&info.audio_url).map_err(|e| e.to_string())?;
            let title = env.new_string(&info.title).map_err(|e| e.to_string())?;
            let class = env
                .find_class("dev/dioxus/main/PlaybackService")
                .map_err(|e| e.to_string())?;
            env.call_static_method(
                class,
                "play",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V",
                &[
                    JValue::Object(&context),
                    JValue::Object(&url),
                    JValue::Object(&title),
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn pause() -> Result<(), String> {
        with_env(|env| call_static(env, "pause", "()V"))
    }

    pub fn resume() -> Result<(), String> {
        with_env(|env| call_static(env, "resume", "()V"))
    }

    pub fn stop() -> Result<(), String> {
        with_env(|env| call_static(env, "stop", "()V"))
    }
}

pub use imp::{pause, play, resume, stop};

#[cfg(test)]
mod tests {
    use super::*;
    use nanopipe_core::StreamInfo;

    #[test]
    fn transport_stubs_are_available_on_non_android_targets() {
        assert!(pause().is_ok());
        assert!(resume().is_ok());
        assert!(stop().is_ok());
    }

    #[test]
    fn play_is_a_noop_off_device() {
        let info = StreamInfo {
            video_id: String::from("abc"),
            title: String::from("t"),
            uploader: String::from("u"),
            thumb_url: None,
            audio_url: String::from("https://x/a.webm"),
            audio_mime: String::from("audio/webm"),
            video_url: None,
            video_mime: None,
        };
        assert!(play(info).is_ok());
    }
}
