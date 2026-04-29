#[cfg(target_os = "macos")]
mod inner {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{AnyThread, DefinedClass, define_class, msg_send};
    use objc2_avf_audio::{
        AVSpeechBoundary, AVSpeechSynthesizer, AVSpeechSynthesizerDelegate, AVSpeechUtterance,
    };
    use objc2_foundation::{
        NSDate, NSDefaultRunLoopMode, NSObject, NSObjectProtocol, NSRange, NSRunLoop, NSString,
    };

    #[derive(Debug)]
    struct SharedState {
        word_index: AtomicUsize,
        finished: AtomicBool,
        generation: AtomicUsize,
    }

    #[derive(Debug)]
    struct DelegateIvars {
        shared: Arc<SharedState>,
        word_offsets: Arc<std::sync::Mutex<Vec<usize>>>,
        epoch: AtomicUsize,
    }

    define_class!(
        #[derive(Debug)]
        #[unsafe(super(NSObject))]
        #[name = "EchoSpeechDelegate"]
        #[ivars = DelegateIvars]
        struct SpeechDelegate;

        unsafe impl NSObjectProtocol for SpeechDelegate {}

        unsafe impl AVSpeechSynthesizerDelegate for SpeechDelegate {
            #[unsafe(method(speechSynthesizer:willSpeakRangeOfSpeechString:utterance:))]
            fn will_speak_range(
                &self,
                _synth: &AVSpeechSynthesizer,
                range: NSRange,
                _utterance: &AVSpeechUtterance,
            ) {
                if self.ivars().epoch.load(Ordering::Relaxed)
                    != self.ivars().shared.generation.load(Ordering::Relaxed)
                {
                    return;
                }
                let char_offset = range.location;
                let offsets = self.ivars().word_offsets.lock().unwrap();
                let word_idx = offsets
                    .iter()
                    .rposition(|&off| off <= char_offset)
                    .unwrap_or(0);
                self.ivars()
                    .shared
                    .word_index
                    .store(word_idx, Ordering::Relaxed);
            }

            #[unsafe(method(speechSynthesizer:didFinishSpeechUtterance:))]
            fn did_finish(&self, _synth: &AVSpeechSynthesizer, _utterance: &AVSpeechUtterance) {
                if self.ivars().epoch.load(Ordering::Relaxed)
                    == self.ivars().shared.generation.load(Ordering::Relaxed)
                {
                    self.ivars().shared.finished.store(true, Ordering::Relaxed);
                }
            }

            #[unsafe(method(speechSynthesizer:didCancelSpeechUtterance:))]
            fn did_cancel(&self, _synth: &AVSpeechSynthesizer, _utterance: &AVSpeechUtterance) {
                // Intentionally empty — cancellation is triggered by stop(),
                // which bumps the generation. We don't want a stale cancel
                // callback to mark a fresh utterance as finished.
            }
        }
    );

    impl SpeechDelegate {
        fn new(
            shared: Arc<SharedState>,
            word_offsets: Arc<std::sync::Mutex<Vec<usize>>>,
        ) -> Retained<Self> {
            let alloc = Self::alloc().set_ivars(DelegateIvars {
                shared,
                word_offsets,
                epoch: AtomicUsize::new(0),
            });
            unsafe { msg_send![super(alloc), init] }
        }
    }

    pub struct Speaker {
        synth: Retained<AVSpeechSynthesizer>,
        delegate: Retained<SpeechDelegate>,
        shared: Arc<SharedState>,
        word_offsets: Arc<std::sync::Mutex<Vec<usize>>>,
        base_word: usize,
    }

    fn wpm_to_rate(wpm: u32) -> f32 {
        let wpm = (wpm as f32).clamp(80.0, 400.0);
        if wpm <= 180.0 {
            (wpm - 80.0) / (180.0 - 80.0) * 0.5
        } else {
            0.5 + (wpm - 180.0) / (400.0 - 180.0) * 0.5
        }
    }

    impl Speaker {
        pub fn new() -> Self {
            let shared = Arc::new(SharedState {
                word_index: AtomicUsize::new(0),
                finished: AtomicBool::new(false),
                generation: AtomicUsize::new(0),
            });
            let word_offsets = Arc::new(std::sync::Mutex::new(Vec::new()));
            let delegate = SpeechDelegate::new(shared.clone(), word_offsets.clone());
            let synth = unsafe { AVSpeechSynthesizer::new() };
            unsafe {
                synth.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            }

            Self {
                synth,
                delegate,
                shared,
                word_offsets,
                base_word: 0,
            }
        }

        pub fn speak(&mut self, words: &[&str], start_idx: usize, wpm: u32) {
            self.stop();
            let tail = &words[start_idx.min(words.len())..];
            if tail.is_empty() {
                return;
            }

            self.base_word = start_idx;

            let mut text = String::new();
            let mut offsets = Vec::with_capacity(tail.len());
            for (i, w) in tail.iter().enumerate() {
                offsets.push(text.len());
                text.push_str(w);
                if i + 1 < tail.len() {
                    text.push(' ');
                }
            }
            *self.word_offsets.lock().unwrap() = offsets;

            // Bump generation so stale callbacks from the previous
            // utterance are ignored.
            let ep = self.shared.generation.fetch_add(1, Ordering::Relaxed) + 1;
            self.delegate.ivars().epoch.store(ep, Ordering::Relaxed);
            self.shared.word_index.store(0, Ordering::Relaxed);
            self.shared.finished.store(false, Ordering::Relaxed);

            let ns_text = NSString::from_str(&text);
            unsafe {
                let utterance =
                    AVSpeechUtterance::initWithString(AVSpeechUtterance::alloc(), &ns_text);
                utterance.setRate(wpm_to_rate(wpm));
                self.synth.speakUtterance(&utterance);
            }
        }

        pub fn stop(&mut self) {
            self.shared.generation.fetch_add(1, Ordering::Relaxed);
            self.shared.finished.store(false, Ordering::Relaxed);
            unsafe {
                self.synth
                    .stopSpeakingAtBoundary(AVSpeechBoundary::Immediate);
            }
        }

        pub fn current_word_index(&self) -> usize {
            self.base_word + self.shared.word_index.load(Ordering::Relaxed)
        }

        pub fn is_finished(&self) -> bool {
            self.shared.finished.load(Ordering::Relaxed)
        }
    }

    impl Drop for Speaker {
        fn drop(&mut self) {
            self.stop();
        }
    }

    pub fn pump() {
        let run_loop = NSRunLoop::currentRunLoop();
        let timeout = NSDate::dateWithTimeIntervalSinceNow(0.005);
        unsafe {
            run_loop.runMode_beforeDate(NSDefaultRunLoopMode, &timeout);
        }
    }
}

#[cfg(target_os = "macos")]
pub use inner::{Speaker, pump};

#[cfg(not(target_os = "macos"))]
mod stub {
    pub struct Speaker;

    impl Speaker {
        pub fn new() -> Self {
            Speaker
        }
        pub fn speak(&mut self, _words: &[&str], _start_idx: usize, _wpm: u32) {}
        pub fn stop(&mut self) {}
        pub fn current_word_index(&self) -> usize {
            0
        }
        pub fn is_finished(&self) -> bool {
            true
        }
    }

    pub fn pump() {}
}

#[cfg(not(target_os = "macos"))]
pub use stub::{Speaker, pump};
