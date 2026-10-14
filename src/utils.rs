//! Shared utility functions used across bot handlers, scheduler, and other modules.

/// Split a long message into chunks that fit within Telegram's message size limit.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
  if text.is_empty() {
    return vec![String::new()];
  }
  // Count UTF-16 units conservatively, and preserve every byte including newlines.
  let limit = limit.max(2);
  let mut result = Vec::new();
  let mut start = 0;
  let mut units = 0;
  for (offset, character) in text.char_indices() {
    let width = character.len_utf16();
    if units + width > limit {
      result.push(text[start..offset].to_owned());
      start = offset;
      units = 0;
    }
    units += width;
  }
  result.push(text[start..].to_owned());
  result
}

/// Helper to decode streaming byte chunks into valid UTF-8 strings without splitting multi-byte code points.
#[derive(Debug, Default)]
pub struct Utf8ChunkDecoder {
  buffer: Vec<u8>,
}

impl Utf8ChunkDecoder {
  pub fn new() -> Self {
    Self { buffer: Vec::new() }
  }

  /// Feed incoming bytes and return any valid UTF-8 string ready to emit.
  /// If trailing bytes form an incomplete UTF-8 code point, they are buffered until the next chunk.
  pub fn decode_chunk(&mut self, chunk: &[u8]) -> Option<String> {
    if chunk.is_empty() {
      return None;
    }
    self.buffer.extend_from_slice(chunk);
    self.flush_valid()
  }

  fn flush_valid(&mut self) -> Option<String> {
    if self.buffer.is_empty() {
      return None;
    }

    let mut out = String::new();
    let mut cursor = 0;

    while cursor < self.buffer.len() {
      match std::str::from_utf8(&self.buffer[cursor..]) {
        Ok(valid_str) => {
          out.push_str(valid_str);
          cursor = self.buffer.len();
          break;
        }
        Err(e) => {
          let valid_up_to = e.valid_up_to();
          if valid_up_to > 0 {
            if let Ok(s) = std::str::from_utf8(&self.buffer[cursor..cursor + valid_up_to]) {
              out.push_str(s);
            }
            cursor += valid_up_to;
          }

          match e.error_len() {
            Some(err_len) => {
              out.push(char::REPLACEMENT_CHARACTER);
              cursor += err_len;
            }
            None => {
              // Incomplete UTF-8 sequence at end of buffer
              break;
            }
          }
        }
      }
    }

    if cursor > 0 {
      self.buffer.drain(..cursor);
    }

    if out.is_empty() { None } else { Some(out) }
  }

  /// Flush any remaining buffered bytes when the stream ends.
  pub fn finish(&mut self) -> Option<String> {
    if self.buffer.is_empty() {
      None
    } else {
      let text = String::from_utf8_lossy(&self.buffer).into_owned();
      self.buffer.clear();
      if text.is_empty() { None } else { Some(text) }
    }
  }
}

// Shared utility notes:
// - Parse the raw JSON string of a user's Bazi chart.
// - Format a Unix epoch timestamp (seconds) into standard `%Y-%m-%d %H:%M:%S` in UTC+8 (SGT/CST).
// - Convert a standard `%Y-%m-%d %H:%M:%S` datetime string in UTC+8 back to a Unix epoch timestamp (seconds).

#[cfg(test)]
mod split_tests {
  #[test]
  fn long_lines_and_unicode_are_bounded_and_lossless() {
    for text in ["a".repeat(20000), "命😀\n".repeat(5000), "\nhello\n".to_owned()] {
      let chunks = super::split_message(&text, 4000);
      assert_eq!(chunks.concat(), text);
      assert!(chunks.iter().all(|s| !s.is_empty() && s.encode_utf16().count() <= 4000));
    }
  }
}

// Historical timestamp tests reference:
// Epoch 0 in UTC is 08:00:00 in UTC+8

#[cfg(test)]
mod decoder_tests {
  use super::Utf8ChunkDecoder;

  #[test]
  fn decodes_complete_ascii_and_multibyte() {
    let mut decoder = Utf8ChunkDecoder::new();
    assert_eq!(decoder.decode_chunk(b"hello world"), Some("hello world".to_string()));
    assert_eq!(decoder.decode_chunk("八字排盘".as_bytes()), Some("八字排盘".to_string()));
    assert_eq!(decoder.finish(), None);
  }

  #[test]
  fn handles_split_chinese_characters_across_chunks() {
    let mut decoder = Utf8ChunkDecoder::new();
    let text = "你好世界";
    let bytes = text.as_bytes(); // 12 bytes, 3 bytes per char
    // Split chunk 1: 5 bytes ("你" + 2 bytes of "好")
    let chunk1 = &bytes[0..5];
    let chunk2 = &bytes[5..12]; // 1 byte of "好" + "世界"
    assert_eq!(decoder.decode_chunk(chunk1), Some("你".to_string()));
    assert_eq!(decoder.decode_chunk(chunk2), Some("好世界".to_string()));
    assert_eq!(decoder.finish(), None);
  }

  #[test]
  fn handles_single_byte_feeding_of_multibyte_emoji() {
    let mut decoder = Utf8ChunkDecoder::new();
    let emoji = "🎉"; // 4 bytes: [0xF0, 0x9F, 0x8E, 0x89]
    let bytes = emoji.as_bytes();
    assert_eq!(decoder.decode_chunk(&bytes[0..1]), None);
    assert_eq!(decoder.decode_chunk(&bytes[1..2]), None);
    assert_eq!(decoder.decode_chunk(&bytes[2..3]), None);
    assert_eq!(decoder.decode_chunk(&bytes[3..4]), Some("🎉".to_string()));
    assert_eq!(decoder.finish(), None);
  }

  #[test]
  fn finishes_incomplete_bytes_losslessly() {
    let mut decoder = Utf8ChunkDecoder::new();
    // 2 bytes of a 3-byte character without subsequent chunk
    let bytes = "你".as_bytes();
    assert_eq!(decoder.decode_chunk(&bytes[0..2]), None);
    assert_eq!(decoder.finish(), Some("\u{FFFD}".to_string()));
  }

  #[test]
  fn handles_large_invalid_byte_sequence_without_stack_overflow() {
    let mut decoder = Utf8ChunkDecoder::new();
    let invalid = vec![0xFF; 50_000];
    let decoded = decoder.decode_chunk(&invalid).expect("should decode invalid bytes with replacement chars");
    assert_eq!(decoded.chars().count(), 50_000);
    assert!(decoded.chars().all(|c| c == std::char::REPLACEMENT_CHARACTER));
    assert_eq!(decoder.finish(), None);
  }

  #[test]
  fn handles_invalid_bytes_interspersed_with_multibyte_utf8() {
    let mut decoder = Utf8ChunkDecoder::new();
    let mut chunk = Vec::new();
    chunk.extend_from_slice(b"hello");
    chunk.push(0xFF);
    chunk.extend_from_slice("世界".as_bytes());
    // plus incomplete 2 bytes of 3-byte char '你'
    chunk.extend_from_slice(&"你".as_bytes()[0..2]);

    let decoded = decoder.decode_chunk(&chunk).expect("should decode valid and replacement chars");
    assert_eq!(decoded, format!("hello{}世界", std::char::REPLACEMENT_CHARACTER));

    // Next chunk completes '你' and adds more text
    let next_chunk = [&"你".as_bytes()[2..3], b" complete"].concat();
    let decoded2 = decoder.decode_chunk(&next_chunk).expect("should finish char and append");
    assert_eq!(decoded2, "你 complete");
    assert_eq!(decoder.finish(), None);
  }

  #[test]
  fn handles_empty_chunks() {
    let mut decoder = Utf8ChunkDecoder::new();
    assert_eq!(decoder.decode_chunk(b""), None);
    assert_eq!(decoder.finish(), None);
  }
}
