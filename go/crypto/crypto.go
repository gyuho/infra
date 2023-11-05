package crypto

import (
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"encoding/base64"
	"encoding/pem"
)

// Returns a new RSA key, the private key in PEM encoding, the public key in base64 encoding.
// If bits is zero, it defaults to 2048.
func NewRSAKey(bits int) (string, string, error) {
	if bits == 0 {
		bits = 4092
	}

	priv, err := rsa.GenerateKey(rand.Reader, bits)
	if err != nil {
		return "", "", err
	}
	privPEM := &pem.Block{
		Type:  "RSA PRIVATE KEY",
		Bytes: x509.MarshalPKCS1PrivateKey(priv),
	}
	privEncoded := string(pem.EncodeToMemory(privPEM))

	pub, err := x509.MarshalPKIXPublicKey(&priv.PublicKey)
	if err != nil {
		return "", "", err
	}
	pubEncoded := base64.StdEncoding.EncodeToString(pub)

	return privEncoded, pubEncoded, nil
}
