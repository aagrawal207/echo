// TTS narration via AVFoundation on macOS.
//
// Speaks the full text as a single utterance. The delegate receives
// word-boundary callbacks (`willSpeakRangeOfSpeechString`) that tell us
// which character offset is about to be spoken. We map that offset back
// to a word index so the RSVP visual can follow along.
//
// The caller must pump the macOS run loop periodically (see `pump()`)
// for callbacks to fire.

#[cfg(target_os = "macos")]
mod inner {
    use std::cell::Cell;
    use std::sync::{Arc, Mutex};

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
    use objc2_avf_audio::{AVSpeechSynthesizer, AVSpeechSynthesizerDelegate, AVSpeechUtterance};
    use objc2_foundation::{
        NSDate, NSDefaultRunLoopMode, NSObject, NSObjectProtocol, NSRange, NSRunLoop, NSString,
    };

    #[derive(Debug, Clone)]
    pub struct TtsState {
        pub word_index: usize,
        pub finished: bool,
    }

    #[derive(Debug)]
    struct DelegateIvars {
        shared: Arc<Mutex<TtsState>>,
        word_offsets: Arc<Mutex<Vec<usize>>>,
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
                let char_offset = range.location;
                let offsets = self.ivars().word_offsets.lock().unwrap();
                let word_idx = offsets
                    .iter()
                    .rposition(|&off| off <= char_offset)
                    .unwrap_or(0);
                let mut state = self.ivars().shared.lock().unwrap();
                state.word_index = word_idx;
            }

            #[unsafe(method(speechSynthesizer:didFinishSpeechUtterance:))]
            fn did_finish(&self, _synth: &AVSpeechSynthesizer, _utterance: &AVSpeechUtterance) {
                let mut state = self.ivars().shared.lock().unwrap();
                state.finished = true;
            }

            #[unsafe(method(speechSynthesizer:didCancelSpeechUtterance:))]
            fn did_cancel(&self, _synth: &AVSpeechSynthesizer, _utterance: &AVSpeechUtterance) {
                let mut state = self.ivars().shared.lock().unwrap();
                state.finished = true;
            }
        }
    );

    impl SpeechDelegate {
        fn new(
            shared: Arc<Mutex<TtsState>>,
            word_offsets: Arc<Mutex<Vec<usize>>>,
        ) -> Retained<Self> {
            let alloc = Self::alloc().set_ivars(DelegateIvars {
                shared,
                word_offsets,
            });
            unsafe { msg_send![super(alloc), init] }
        }
    }

    pub struct Speaker {
        synth: Retained<AVSpeechSynthesizer>,
        _delegate: Retained<SpeechDelegate>,
        shared: Arc<Mutex<TtsState>>,
        word_offsets: Arc<Mutex<Vec<usize>>>,
        base_word: usize,
    }

    fn wpm_to_rate(wpm: u32) -> f32 {
        // AVSpeechUtterance rate: 0.0 (slowest) to 1.0 (fastest),
        // default 0.5 (~180 wpm). Rough linear mapping.
        let clamped = (wpm as f32).clamp(60.0, 600.0);
        ((clamped - 60.0) / (600.0 - 60.0)).clamp(0.0, 1.0)
    }

    impl Speaker {
        pub fn new() -> Self {
            let shared = Arc::new(Mutex::new(TtsState {
                word_index: 0,
                finished: false,
            }));
            let word_offsets = Arc::new(Mutex::new(Vec::new()));
            let delegate = SpeechDelegate::new(shared.clone(), word_offsets.clone());
            let synth = unsafe { AVSpeechSynthesizer::new() };
            unsafe {
                synth.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            }
            Self {
                synth,
                _delegate: delegate,
                shared,
                word_offsets,
                base_word: 0,
            }
        }

        pub fn speak(&mut self, words: &[&str], start_idx: usize, wpm: u32) {
            self.stop();
            if words.is_empty() {
                return;
            }

            self.base_word = start_idx;

            // Build the text and record each word's char offset.
            let mut text = String::new();
            let mut offsets = Vec::with_capacity(words.len());
            for (i, w) in words.iter().enumerate() {
                offsets.push(text.len());
                text.push_str(w);
                if i + 1 < words.len() {
                    text.push(' ');
                }
            }
            *self.word_offsets.lock().unwrap() = offsets;
            {
                let mut state = self.shared.lock().unwrap();
                state.word_index = 0;
                state.finished = false;
            }

            let ns_text = NSString::from_str(&text);
            unsafe {
                let utterance =
                    AVSpeechUtterance::initWithString(AVSpeechUtterance::alloc(), &ns_text);
                utterance.setRate(wpm_to_rate(wpm));
                self.synth.speakUtterance(&utterance);
            }
        }

        pub fn stop(&mut self) {
            unsafe {
                self.synth
                    .stopSpeakingAtBoundary(objc2_avf_audio::AVSpeechBoundary::Immediate);
            }
            let mut state = self.shared.lock().unwrap();
            state.finished = true;
        }

        pub fn state(&self) -> TtsState {
            self.shared.lock().unwrap().clone()
        }

        pub fn current_word_index(&self) -> usize {
            self.base_word + self.shared.lock().unwrap().word_index
        }

        pub fn is_finished(&self) -> bool {
            self.shared.lock().unwrap().finished
        }
    }

    impl Drop for Speaker {
        fn drop(&mut self) {
            self.stop();
        }
    }

    pub fn pump() {
        let run_loop = NSRunLoop::currentRunLoop();
        let timeout = unsafe { NSDate::dateWithTimeIntervalSinceNow(0.005) };
        unsafe {
            run_loop.runMode_beforeDate(NSDefaultRunLoopMode, &timeout);
        }
    }
}

#[cfg(target_os = "macos")]
pub use inner::{Speaker, TtsState, pump};

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
