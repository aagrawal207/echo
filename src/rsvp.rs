use std::io::{self, Write};
use std::thread;
use std::time::Duration;

pub fn play(text: &str, wpm: u32) -> io::Result<()> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(());
    }

    let per_word = Duration::from_millis(60_000 / u64::from(wpm));
    let mut stdout = io::stdout().lock();

    for word in words {
        write!(stdout, "\r\x1b[K{word}")?;
        stdout.flush()?;
        thread::sleep(per_word);
    }
    writeln!(stdout)?;
    Ok(())
}
