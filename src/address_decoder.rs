use std::error::Error;
use std::io;

use k256::sha2::Digest;
use k256::sha2::Sha256;

/// Recibe la address comprimida
/// Devuelve el PubkeyHash
/// Si la address es invalida, devuelve error
pub fn get_pubkey_hash_from_address(address: &str) -> Result<[u8; 20], Box<dyn Error>> {
    //se decodifican de &str a bytes , desde el formate base58  a bytes
    let address_decoded_bytes = bs58::decode(address).into_vec()?;
    validate_address(&address_decoded_bytes)?;
    let lenght_bytes = address_decoded_bytes.len();
    let mut pubkey_hash: [u8; 20] = [0; 20];

    // el pubkey hash es el que compone la address
    // le saco el byte de la red y el checksum del final
    pubkey_hash.copy_from_slice(&address_decoded_bytes[1..(lenght_bytes - 4)]);

    Ok(pubkey_hash)
}

/// Recibe una bitcoin address decodificada.
/// Revisa el checksum y devuelve error si es inválida.
fn validate_address(address_decoded_bytes: &Vec<u8>) -> Result<(), Box<dyn Error>> {
    // validacion checksum: evita errores de tipeo en la address
    // Calcular el checksum (doble hash SHA-256) del hash extendido
    let lenght_bytes = address_decoded_bytes.len();
    let checksum_hash = Sha256::digest(Sha256::digest(
        &address_decoded_bytes[0..(lenght_bytes - 4)],
    ));

    let checksum_address = &address_decoded_bytes[(lenght_bytes - 4)..lenght_bytes];
    if checksum_address != &checksum_hash[..4] {
        return Err(Box::new(std::io::Error::new(
            io::ErrorKind::Other,
            "La dirección es inválida, falló la validación del checksum",
        )));
    }
    Ok(())
}

#[cfg(test)]

mod test {
    use std::error::Error;

    use super::get_pubkey_hash_from_address;
    use crate::account::Account;
    use bitcoin_hashes::{ripemd160, Hash};
    use k256::sha2::Digest;
    use k256::sha2::Sha256;
    use secp256k1::SecretKey;

    fn generate_pubkey_hash(private_key: &[u8]) -> [u8; 20] {
        let secp: secp256k1::Secp256k1<secp256k1::All> = secp256k1::Secp256k1::new();
        let key: SecretKey = SecretKey::from_slice(private_key).unwrap();
        let public_key: secp256k1::PublicKey = secp256k1::PublicKey::from_secret_key(&secp, &key);
        //  se aplica RIPEMD160(SHA256(ECDSA(public_key)))
        let public_key_compressed = public_key.serialize();
        // let pk_hex: String = public_key_hexa.encode_hex::<String>();

        // Aplica hash160
        let sha256_hash = Sha256::digest(public_key_compressed);
        let ripemd160_hash = *ripemd160::Hash::hash(&sha256_hash).as_byte_array();
        ripemd160_hash
    }

    #[test]
    fn test_decodificacion_de_address_valida_devuelve_ok() {
        let address = "mpzx6iZ1WX8hLSeDRKdkLatXXPN1GDWVaF";
        let pubkey_hash_expected = get_pubkey_hash_from_address(address);
        assert!(pubkey_hash_expected.is_ok())
    }

    #[test]
    fn test_decodificacion_de_adress_genera_pubkey_esperado() -> Result<(), Box<dyn Error>> {
        let address: &str = "mnEvYsxexfDEkCx2YLEfzhjrwKKcyAhMqV";
        let private_key: &str = "cMoBjaYS6EraKLNqrNN8DvN93Nnt6pJNfWkYM8pUufYQB5EVZ7SR";
        let private_key_bytes = Account::decode_wif_private_key(private_key)?;
        let pubkey_hash_expected = generate_pubkey_hash(&private_key_bytes);
        let pubkey_hash_generated = get_pubkey_hash_from_address(address)?;
        assert_eq!(pubkey_hash_expected, pubkey_hash_generated);
        Ok(())
    }

    #[test]
    fn test_pub_key_hash_se_genera_con_el_largo_correcto() -> Result<(), Box<dyn Error>> {
        let address = "mnEvYsxexfDEkCx2YLEfzhjrwKKcyAhMqV";
        let pub_key_hash = get_pubkey_hash_from_address(address)?;

        assert_eq!(pub_key_hash.len(), 20);
        Ok(())
    }
    #[test]
    fn test_get_pubkey_hash_con_direccion_invalida_da_error() -> Result<(), Box<dyn Error>> {
        let address = "1nEvYsxexfDEkCx2YLEfzhjrwKKcyAhMqV";
        let pub_key_hash_result = get_pubkey_hash_from_address(address);

        assert!(pub_key_hash_result.is_err());
        Ok(())
    }
}
