use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};

use crate::config::Config;
const NUMBER_OF_NODES: usize = 8;

/// Devuelve una lista de direcciones Ipv4 obtenidas del dns seed
pub fn get_active_nodes_from_dns_seed(config: &Config) -> std::io::Result<Vec<Ipv4Addr>> {
    let mut node_ips = Vec::new();
    let host = "seed.testnet.bitcoin.sprovoost.nl";
    let port = 8333;

    let addrs = match (host, port).to_socket_addrs() {
        Ok(addrs) => addrs,
        Err(e) => {
            //   println!("Error al resolver {}: {}", host, e);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No se pudo conectar a la DNS seed!\n",
            ));
        }
    };
    for addr in addrs {
        if let SocketAddr::V4(v4_addr) = addr {
            node_ips.push(*v4_addr.ip());
        }
    }
    Ok(node_ips)
}
/*
#[cfg(test)]
mod tests {
    use super::*;
    fn is_valid_ip_address(ip_address: &str) -> bool {
        let octets: Vec<&str> = ip_address.split('.').collect();

        if octets.len() != 4 {
            return false;
        }

        for octet in octets {
            match octet.parse::<u8>() {
                Ok(_) => continue,
                _ => return false,
            }
        }

        true
    }
    #[test]
    fn getting_number_of_nodes_correctly_with_one_valid_dns_seed_direction() {
        let valid_dns = "seed.testnet.bitcoin.sprovoost.nl".to_string();
        let active_nodes = get_active_nodes_from_dns_seed(valid_dns).unwrap();
        assert_eq!(NUMBER_OF_NODES, active_nodes.len())
    }
    #[test]
    fn getting_number_of_nodes_correctly_with_other_valid_dns_seed_direction() {
        let valid_dns = "testnet-seed.bitcoin.jonasschnelli.ch".to_string();
        let active_nodes = get_active_nodes_from_dns_seed(valid_dns).unwrap();
        assert_eq!(NUMBER_OF_NODES, active_nodes.len())
    }
    #[test]
    fn getting_ip_addresses_correctly_with_valid_dns_seed_direction() {
        let valid_dns = "testnet-seed.bitcoin.jonasschnelli.ch".to_string();
        let active_nodes = get_active_nodes_from_dns_seed(valid_dns).unwrap();
        for node in active_nodes {
            assert!(is_valid_ip_address(node.as_str()))
        }
    }
    #[test]
    fn getting_error_with_invalid_dns_seed_direction() {
        let invalid_dns = "invalid_dns_seed".to_string();
        let active_nodes = get_active_nodes_from_dns_seed(invalid_dns);
        assert!(active_nodes.is_err());
        assert_eq!(
            active_nodes.unwrap_err().to_string(),
            "No se obtuvieron la cantidad necesaria de nodos de la DNS seed! \n"
        )
    }
}
*/
