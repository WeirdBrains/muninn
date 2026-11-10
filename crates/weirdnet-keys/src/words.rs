//! The 24 BIP39 words for a 256-bit key.
use crate::error::{KeysError, Result};
use crate::key::Key32;
use zeroize::Zeroizing;

/// 24 words encoding the key's 256 bits.
pub fn to_words(master: &Key32) -> Result<Zeroizing<String>> {
    let m = bip39::Mnemonic::from_entropy(&master.0).map_err(|e| KeysError::crypto(e.to_string()))?;
    Ok(Zeroizing::new(m.to_string()))
}

/// The key from its 24 words (any case and spacing; the BIP39 checksum is checked).
pub fn from_words(words: &str) -> Result<Key32> {
    let norm = Zeroizing::new(words.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase());
    let m = bip39::Mnemonic::parse(norm.as_str()).map_err(|e| KeysError::crypto(format!("invalid recovery words: {e}")))?;
    if m.word_count() != 24 {
        return Err(KeysError::crypto(format!("expected 24 words, got {}", m.word_count())));
    }
    let e = Zeroizing::new(m.to_entropy());
    Key32::from_slice(&e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_round_trip_and_checksum() {
        let k = Key32::random().unwrap();
        let w = to_words(&k).unwrap();
        assert_eq!(w.split_whitespace().count(), 24);
        assert_eq!(from_words(&w.to_uppercase()).unwrap(), k);
        assert!(from_words("abandon abandon").is_err());
        // swapping two words breaks the checksum (or, rarely, gives another key)
        let mut v: Vec<&str> = w.split_whitespace().collect();
        v.swap(0, 23);
        assert!(from_words(&v.join(" ")).map(|x| x != k).unwrap_or(true));
    }
}
