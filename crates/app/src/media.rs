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
    use jni::objects::{JClass, JObject, JValue};
    use jni::JavaVM;
    use nanopipe_core::StreamInfo;

    // Resolve the service class through the *app* class loader. JNIEnv::find_class
    // resolves against the system loader on a natively attached thread, which cannot
    // see app classes, so it would fail with ClassNotFoundException.
    fn service_class<'a>(env: &mut jni::JNIEnv<'a>) -> Result<JClass<'a>, String> {
        let ctx = ndk_context::android_context();
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let loader = env
            .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .map_err(|e| format!("getClassLoader: {e}"))?
            .l()
            .map_err(|e| format!("getClassLoader: {e}"))?;
        let name = env
            .new_string("dev.dioxus.main.PlaybackService")
            .map_err(|e| format!("new_string: {e}"))?;
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )
            .map_err(|e| format!("loadClass: {e}"))?
            .l()
            .map_err(|e| format!("loadClass: {e}"))?;
        Ok(unsafe { JClass::from_raw(class.into_raw()) })
    }

    fn clear(env: &mut jni::JNIEnv) {
        if let Ok(true) = env.exception_check() {
            let _ = env.exception_clear();
        }
    }

    fn with_env<F>(f: F) -> Result<(), String>
    where
        F: FnOnce(&mut jni::JNIEnv) -> Result<(), String>,
    {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let result = f(&mut env);
        if result.is_err() {
            clear(&mut env);
        }
        result
    }

    pub fn play(info: StreamInfo) -> Result<(), String> {
        with_env(|env| {
            let ctx = ndk_context::android_context();
            let context = unsafe { JObject::from_raw(ctx.context().cast()) };
            let url = env
                .new_string(&info.audio_url)
                .map_err(|e| format!("new_string: {e}"))?;
            let title = env
                .new_string(&info.title)
                .map_err(|e| format!("new_string: {e}"))?;
            let agent = env
                .new_string(&info.user_agent)
                .map_err(|e| format!("new_string: {e}"))?;
            let dash = env
                .new_string(&info.dash_manifest())
                .map_err(|e| format!("new_string: {e}"))?;
            let class = service_class(env)?;
            env.call_static_method(
                class,
                "play",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
                &[
                    JValue::Object(&context),
                    JValue::Object(&url),
                    JValue::Object(&title),
                    JValue::Object(&agent),
                    JValue::Object(&dash),
                ],
            )
            .map(|_| ())
            .map_err(|e| format!("PlaybackService.play: {e}"))
        })
    }

    pub fn pause() -> Result<(), String> {
        with_env(|env| {
            let class = service_class(env)?;
            env.call_static_method(class, "pause", "()V", &[])
                .map(|_| ())
                .map_err(|e| format!("PlaybackService.pause: {e}"))
        })
    }

    pub fn resume() -> Result<(), String> {
        with_env(|env| {
            let class = service_class(env)?;
            env.call_static_method(class, "resume", "()V", &[])
                .map(|_| ())
                .map_err(|e| format!("PlaybackService.resume: {e}"))
        })
    }

    pub fn stop() -> Result<(), String> {
        with_env(|env| {
            let class = service_class(env)?;
            env.call_static_method(class, "stop", "()V", &[])
                .map(|_| ())
                .map_err(|e| format!("PlaybackService.stop: {e}"))
        })
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
            audio_codec: String::from("opus"),
            audio_bitrate: 128_000,
            audio_index_range: Some((200, 399)),
            audio_init_range: Some((0, 199)),
            duration_ms: Some(1000),
            audio_size: Some(2_806_189),
            video_url: None,
            video_mime: None,
            user_agent: String::from("test-agent"),
        };
        assert!(play(info).is_ok());
    }
}
