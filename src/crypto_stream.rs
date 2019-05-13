use crypto::salsa20::Salsa20;
use crypto::symmetriccipher::SynchronousStreamCipher;

// libsodium does not expose such an interface, so here it is.
pub(crate) fn crypto_stream_xor_instance(nonce: &[u8], key: &[u8]) -> Xor {
    Xor(Salsa20::new_xsalsa20(key, nonce))
}

pub(crate) struct Xor(Salsa20);

impl Xor {
    pub(crate) fn update(&mut self, input: &[u8], output: &mut [u8]) {
        self.0.process(input, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn crypto_stream_1() {
        let nonce = b"012345678901234567890123";
        let key = b"01234567890123456789012345678901";
        let mut xor = crypto_stream_xor_instance(nonce, key);
        let input = b"foo";
        let mut output = vec![0u8; 3];
        xor.update(input, output.as_mut_slice());
        assert_eq!(data_encoding::HEXUPPER.encode(&output), "51C634");
    }

    #[test]
    fn crypto_stream_2() {
        let nonce = b"012345678901234567890123";
        let key = b"01234567890123456789012345678901";
        let mut xor = crypto_stream_xor_instance(nonce, key);
        let input = b"foo";
        let mut output = vec![0u8; 3];
        xor.update(input, output.as_mut_slice());
        assert_eq!(data_encoding::HEXUPPER.encode(&output), "51C634");

        xor.update(b"bar", output.as_mut_slice());
        assert_eq!(data_encoding::HEXUPPER.encode(&output), "8FC158");
    }

    #[test]
    fn crypto_stream_3() {
        sodiumoxide::init().unwrap();
        let mut nonce = [0u8; 24];
        sodiumoxide::randombytes::randombytes_into(&mut nonce);
        let mut key = [0u8; 32];
        sodiumoxide::randombytes::randombytes_into(&mut key);

        let mut rng = rand::thread_rng();
        let buf_size = rng.gen_range(0, 1024 * 1024);
        let buf = sodiumoxide::randombytes::randombytes(buf_size);

        // split up buf into chunks
        let mut chunks = Vec::new();
        let mut remaining_bytes = &buf[..];
        while !remaining_bytes.is_empty() {
            let chunk_size = rng.gen_range(1, remaining_bytes.len() + 1);
            chunks.push(&remaining_bytes[..chunk_size]);
            remaining_bytes = &remaining_bytes[chunk_size..];
        }
        assert_eq!(chunks.concat(), buf);

        // encrypt chunks
        let mut xor = crypto_stream_xor_instance(&nonce, &key);
        let mut result_chunks = Vec::new();
        for chunk in chunks {
            let mut output = vec![0u8; chunk.len()];
            xor.update(chunk, output.as_mut_slice());
            result_chunks.push(output);
        }

        let result = result_chunks.concat();
        let expected = sodiumoxide::crypto::stream::stream_xor(
            &buf,
            &sodiumoxide::crypto::stream::Nonce(nonce),
            &sodiumoxide::crypto::stream::Key(key),
        );
        assert_eq!(result, expected);
    }
}
