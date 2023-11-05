// Package envelope implements envelope encryption for AWS KMS.
// ref. https://github.com/gyuho/infra/blob/main/aws/rust/src/kms/envelope.rs
package envelope

import (
	"bytes"
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"encoding/binary"
	"fmt"
	"io"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_kms_v2 "github.com/aws/aws-sdk-go-v2/service/kms"
	aws_kms_v2_types "github.com/aws/aws-sdk-go-v2/service/kms/types"
	"github.com/dustin/go-humanize"
)

const (
	// ref. https://pkg.go.dev/crypto/cipher#NewGCM
	DEK_AES_256_LENGTH = 32
	NONCE_LEN          = 12
)

// Envelope-encrypts the data using AWS KMS data-encryption key (DEK) and "AES_256_GCM".
// AWS Encrypt API can only encrypt up to 4KB of data, so we need to use envelope encryption
// to encrypt larger data.
// "aadTag" is the additional authenticated data (AAD) tag that attaches information
// to the ciphertext that is not encrypted.
//
// The encrypted data are aligned as below:
// [ Nonce bytes "length" ][ DEK.ciphertext "length" ][ Nonce bytes ][ DEK.ciphertext ][ data ciphertext ]
//
// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Decrypt.html
func SealAES256(
	ctx context.Context,
	cfg aws.Config,
	keyID string,
	plaintext []byte,
	aadTag []byte,
) ([]byte, error) {
	logutil.S().Infow("AES_256 envelope-encrypting data",
		"keyID", keyID,
		"sizeBeforeEncryption", humanize.Bytes(uint64(len(plaintext))),
	)

	nonce := make([]byte, NONCE_LEN)
	if _, err := io.ReadFull(rand.Reader, nonce); err != nil {
		return nil, err
	}

	cli := aws_kms_v2.NewFromConfig(cfg)
	dek, err := cli.GenerateDataKey(ctx, &aws_kms_v2.GenerateDataKeyInput{
		KeyId:   &keyID,
		KeySpec: aws_kms_v2_types.DataKeySpecAes256,
	})
	if err != nil {
		return nil, err
	}
	if len(dek.Plaintext) != DEK_AES_256_LENGTH {
		return nil, fmt.Errorf("DEK.plaintext for AES_256 must be %d bytes, got %d", DEK_AES_256_LENGTH, len(dek.Plaintext))
	}

	block, err := aes.NewCipher(dek.Plaintext)
	if err != nil {
		return nil, err
	}
	aesgcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	// align bytes in the order of
	// - Nonce bytes "length"
	// - DEK.ciphertext "length"
	// - Nonce bytes
	// - DEK.ciphertext
	// - data ciphertext
	dst := new(bytes.Buffer)

	// Nonce bytes "length"
	if err := binary.Write(dst, binary.LittleEndian, uint16(NONCE_LEN)); err != nil {
		return nil, err
	}

	// DEK.ciphertext "length"
	if err := binary.Write(dst, binary.LittleEndian, uint16(len(dek.CiphertextBlob))); err != nil {
		return nil, err
	}

	// Nonce bytes
	if _, err := dst.Write(nonce); err != nil {
		return nil, err
	}

	// DEK.ciphertext
	if _, err := dst.Write(dek.CiphertextBlob); err != nil {
		return nil, err
	}

	ciphertext := aesgcm.Seal(dst.Bytes(), nonce, plaintext, aadTag)
	logutil.S().Infow("AES_256 envelope-encrypted data", "sizeAfterEncryption", humanize.Bytes(uint64(len(ciphertext))))

	return ciphertext, nil
}

// Envelope-decrypts using KMS DEK and "AES_256_GCM".
//
// Assume the input (ciphertext) data are packed in the order of:
// [ Nonce bytes "length" ][ DEK.ciphertext "length" ][ Nonce bytes ][ DEK.ciphertext ][ data ciphertext ]
//
// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Encrypt.html
// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_GenerateDataKey.html
func UnsealAES256(
	ctx context.Context,
	cfg aws.Config,
	keyID string,
	ciphertext []byte,
	aadTag []byte,
) ([]byte, error) {
	logutil.S().Infow("AES_256 envelope-decrypting data",
		"keyID", keyID,
		"sizeBeforeDecryption", humanize.Bytes(uint64(len(ciphertext))),
	)

	// bytes are packed in the order of
	// - Nonce bytes "length"
	// - DEK.ciphertext "length"
	// - Nonce bytes
	// - DEK.ciphertext
	// - data ciphertext
	cur := bytes.NewReader(ciphertext)

	// Nonce bytes "length"
	nonceLenBytes := make([]byte, 2)
	if err := binary.Read(cur, binary.LittleEndian, nonceLenBytes); err != nil {
		return nil, err
	}
	nonceLen := int(binary.LittleEndian.Uint16(nonceLenBytes))
	if nonceLen != NONCE_LEN {
		return nil, fmt.Errorf("nonce length must be %d bytes, got %d", NONCE_LEN, nonceLen)
	}

	// DEK.ciphertext "length"
	dekCiphertextLenBytes := make([]byte, 2)
	if err := binary.Read(cur, binary.LittleEndian, dekCiphertextLenBytes); err != nil {
		return nil, err
	}
	dekCiphertextLen := int(binary.LittleEndian.Uint16(dekCiphertextLenBytes))
	if dekCiphertextLen > len(ciphertext) {
		return nil, fmt.Errorf("DEK.ciphertext length must be less than ciphertext %d bytes, got %d", len(ciphertext), dekCiphertextLen)
	}

	// Nonce bytes
	nonce := make([]byte, NONCE_LEN)
	n, err := cur.Read(nonce)
	if err != nil {
		return nil, err
	}
	if n != NONCE_LEN {
		return nil, fmt.Errorf("read nonce bytes must be %d bytes, got %d", NONCE_LEN, n)
	}

	// DEK.ciphertext
	dekCiphertext := make([]byte, dekCiphertextLen)
	n, err = cur.Read(dekCiphertext)
	if err != nil {
		return nil, err
	}
	if n != dekCiphertextLen {
		return nil, fmt.Errorf("read cipher bytes must be %d bytes, got %d", dekCiphertextLen, n)
	}

	cli := aws_kms_v2.NewFromConfig(cfg)
	dek, err := cli.Decrypt(ctx, &aws_kms_v2.DecryptInput{
		CiphertextBlob:      dekCiphertext,
		EncryptionAlgorithm: aws_kms_v2_types.EncryptionAlgorithmSpecSymmetricDefault,
		KeyId:               &keyID,
	})
	if err != nil {
		return nil, err
	}

	block, err := aes.NewCipher(dek.Plaintext)
	if err != nil {
		return nil, err
	}
	aesgcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	// data ciphertext
	cipher, err := io.ReadAll(cur)
	if err != nil {
		return nil, err
	}

	decrypted, err := aesgcm.Open(nil, nonce, cipher, aadTag)
	if err != nil {
		return nil, err
	}
	logutil.S().Infow("AES_256 envelope-decrypted data", "sizeAfterDecryption", humanize.Bytes(uint64(len(decrypted))))

	return decrypted, nil
}
