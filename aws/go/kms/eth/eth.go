package eth

import (
	"encoding/asn1"

	"github.com/ethereum/go-ethereum/crypto"
)

// ref. https://github.com/welthee/go-ethereum-aws-kms-tx-signer
type asn1EcPublicKey struct {
	EcPublicKeyInfo asn1EcPublicKeyInfo
	PublicKey       asn1.BitString
}

// ref. https://github.com/welthee/go-ethereum-aws-kms-tx-signer
type asn1EcPublicKeyInfo struct {
	Algorithm  asn1.ObjectIdentifier
	Parameters asn1.ObjectIdentifier
}

// ref. https://github.com/welthee/go-ethereum-aws-kms-tx-signer
func DeriveAddress(pubKey []byte) (string, error) {
	// get public key DER bytes from KMS
	var asn1pubk asn1EcPublicKey
	if _, err := asn1.Unmarshal(pubKey, &asn1pubk); err != nil {
		return "", err
	}

	decoded, err := crypto.UnmarshalPubkey(asn1pubk.PublicKey.Bytes)
	if err != nil {
		return "", err
	}

	ethereumAddress := crypto.PubkeyToAddress(*decoded).Hex()
	return ethereumAddress, nil
}
