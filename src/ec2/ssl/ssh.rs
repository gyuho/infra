use std::io;

use aws_smithy_types::base64;
use openssl::rsa::Rsa;

/// Returns a new RSA key, the private key in PEM encoding, the public key in base64 encoding.
pub fn new_rsa_key() -> io::Result<(String, String)> {
    let generated_key = Rsa::generate(4096).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to rsa generate {}", e),
        )
    })?;

    let pk = generated_key.private_key_to_pem().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to derive rsa private key to pem {}", e),
        )
    })?;
    let pk_pem_encoded = String::from_utf8(pk).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to convert rsa private key to string {}", e),
        )
    })?;

    // ref. <https://www.ietf.org/rfc/rfc4251.txt>
    let pubkey = generated_key.public_key_to_der().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to derive rsa public key to der {}", e),
        )
    })?;
    // do not prefix with "ssh-rsa "
    // otherwise, import ec2 key pair will fail with
    // "Key is not in valid OpenSSH public key format"
    let pubkey_der_encoded = base64::encode(pubkey);

    Ok((pk_pem_encoded, pubkey_der_encoded))
}

/// RUST_LOG=debug cargo test --package aws-manager --lib -- ec2::ssl::test_ssh --exact --show-output
#[test]
fn test_ssh() {
    let (pk_encoded, pubkey_encoded) = new_rsa_key().unwrap();
    println!("{pk_encoded}");
    println!("{pubkey_encoded}");
}
