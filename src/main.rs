use bitcoin::config::Config;
use bitcoin::gtk::gtk::Gtk;
use bitcoin::handler::node_message_handler::NodeMessageHandlerError;
use bitcoin::handshake::{HandShakeError, Handshake};
use bitcoin::initial_block_download::{initial_block_download, DownloadError};
use bitcoin::logwriter::log_writer::{
    set_up_loggers, shutdown_loggers, write_in_log, LogSender, LoggingError,
};
use bitcoin::network::{get_active_nodes_from_dns_seed, ConnectionToDnsError};
use bitcoin::node::Node;
use bitcoin::wallet::Wallet;
use gtk::glib::ParamSpec;
use std::error::Error;
use std::num::ParseIntError;
use std::sync::{Arc, RwLock};
use std::{env, fmt};

#[derive(Debug)]
pub enum GenericError {
    DownloadError(DownloadError),
    HandShakeError(HandShakeError),
    ConfigError(Box<dyn Error>),
    ConnectionToDnsError(ConnectionToDnsError),
    LoggingError(LoggingError),
    NodeHandlerError(NodeMessageHandlerError),
}

impl fmt::Display for GenericError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GenericError::DownloadError(msg) => write!(f, "DOWNLOAD ERROR: {}", msg),
            GenericError::ConfigError(msg) => write!(f, "CONFIG ERROR: {}", msg),
            GenericError::HandShakeError(msg) => write!(f, "HANDSHAKE ERROR: {}", msg),
            GenericError::ConnectionToDnsError(msg) => {
                write!(f, "CONNECTION TO DNS ERROR: {}", msg)
            }
            GenericError::LoggingError(msg) => write!(f, "LOGGING ERROR: {}", msg),
            GenericError::NodeHandlerError(msg) => {
                write!(f, "NODE MESSAGE LISTENER AND WRITER ERROR: {}", msg)
            }
        }
    }
}

impl Error for GenericError {}

fn main() -> Result<(), GenericError> {
    let mut args: Vec<String> = env::args().collect();
    if args.len() == 3 && args[2] == *"-i" {
        Gtk::run();
        // lo saco para que lea config correctamente
        args.pop();
    }
    let config: Arc<Config> = Config::from(&args).map_err(GenericError::ConfigError)?;
    let (
        error_log_sender,
        error_handler,
        info_log_sender,
        info_handler,
        message_log_sender,
        message_handler,
    ) = set_up_loggers(
        config.clone(),
        config.error_log_path.clone(),
        config.info_log_path.clone(),
        config.message_log_path.clone(),
    )
    .map_err(GenericError::LoggingError)?;
    let logsender = LogSender::new(error_log_sender, info_log_sender, message_log_sender);
    write_in_log(
        logsender.info_log_sender.clone(),
        "Se leyo correctamente el archivo de configuracion\n",
    );
    let active_nodes = get_active_nodes_from_dns_seed(config.clone(), logsender.clone())
        .map_err(GenericError::ConnectionToDnsError)?;
    let sockets = Handshake::handshake(config.clone(), logsender.clone(), &active_nodes)
        .map_err(GenericError::HandShakeError)?;
    // Acá iría la descarga de los headers

    let pointer_to_nodes = Arc::new(RwLock::new(sockets));

    let headers_and_blocks =
        initial_block_download(config, logsender.clone(), pointer_to_nodes.clone()).map_err(
            |err| {
                write_in_log(
                    logsender.error_log_sender.clone(),
                    format!("Error al descargar los bloques: {}", err).as_str(),
                );
                GenericError::DownloadError(err)
            },
        )?;
    let (headers, blocks) = headers_and_blocks;

    let node = Node::new(logsender.clone(), pointer_to_nodes, headers, blocks)
        .map_err(GenericError::NodeHandlerError)?;
    let mut wallet = Wallet::new(node.clone()).map_err(GenericError::NodeHandlerError)?;

/* 
    wallet
        .add_account(
            "cSqmqW48wCeoUF8FCJvVsqUGwcvir27bKWCFj1MTFszFdn2Dduim".to_string(),
            "mocD12x6BV3qK71FwG98h5VWZ4qVsbaoi9".to_string(),
        )
        .map_err(GenericError::NodeHandlerError)?;
    wallet
        .add_account(
            "cSVpNr93PCFhizA9ELgnmkwRxycL1bn6vx1WBJ7SmE8ve9Aq1PzZ".to_string(),
            "mmkNBGEEzj7ePpDii91zgUXi3i3Hgkpi9a".to_string(),
        )
        .map_err(GenericError::NodeHandlerError)?;

    match wallet.make_transaction_index(0, "mmkNBGEEzj7ePpDii91zgUXi3i3Hgkpi9a", 10000, 2000) {
        Ok(_) => println!("Transaccion ok"),
        Err(e) => println!("Error al realizar la transaccion: {}", e),
    }

*/
    if let Err(err) = handle_input(wallet) {
        println!("Error al leer la entrada por terminal. {}", err);
    }

    node.shutdown_node().map_err(GenericError::NodeHandlerError)?;

    write_in_log(
        logsender.info_log_sender.clone(),
        "TERMINA CORRECTAMENTE EL PROGRAMA!",
    );
    shutdown_loggers(logsender, error_handler, info_handler, message_handler)
        .map_err(GenericError::LoggingError)?;

    Ok(())
}




fn handle_input(mut wallet: Wallet) -> Result<(), GenericError> {
    show_options();
    loop {
        let mut input = String::new();

        match std::io::stdin().read_line(&mut input) {
            Ok(_) => {
                println!("\n\n");
                let command = input.trim();
                if let Ok(num) = command.parse::<u32>() {
                    match num {
                        0 => {
                            println!("Cerrando nodo...\n");
                            break;
                        }
                        1 => {
                            handle_add_account_request(&mut wallet);
                        }
                        2 => {
                            handle_balance_request(&mut wallet);
                        }
                        3 => {
                            handle_transaccion_request(&mut wallet);
                        }
                        _ => {
                            println!("Número no reconocido. Inténtalo de nuevo! \n");
                        }
                    }
                    show_options();
                } else {
                    println!("Entrada inválida. Inténtalo de nuevo! \n");
                }
            }
            Err(error) => {
                println!("Error al leer la entrada: {}", error);
            }
        }
    }

    Ok(())
}

fn show_options() {
    println!("\n");
    println!("INGRESE ALGUNO DE LOS SIGUIENTES COMANDOS\n");
    println!("0: terminar el programa");
    println!("1: añadir una cuenta a la wallet");
    println!("2: mostrar balance de las cuentas");
    println!("3: hacer transaccion desde una cuenta");
    println!("4: prueba de inclusion de una transaccion en un bloque");
    println!("-----------------------------------------------------------\n");
}




fn handle_transaccion_request(wallet: &mut Wallet) {
    println!("INGRESE LOS SIGUIENTES DATOS PARA REALIZAR UNA TRANSACCION \n");  
    println!("Índice de la cuenta:");  
    let mut account_index_input = String::new();
    match std::io::stdin().read_line(&mut account_index_input) {
        Ok(_) => {
            let account_index = account_index_input.trim().parse::<usize>();
            match account_index {
                Ok(index) => {
                    println!("Dirección del receptor:");
                    let mut address_receiver_input = String::new();
                    match std::io::stdin().read_line(&mut address_receiver_input) {
                        Ok(_) => {
                            let address_receiver = address_receiver_input.trim();

                            println!("Cantidad:");
                            let mut amount_input = String::new();
                            match std::io::stdin().read_line(&mut amount_input) {
                                Ok(_) => {
                                    let amount = amount_input.trim().parse::<i64>();
                                    match amount {
                                        Ok(parsed_amount) => {
                                            println!("Tarifa:");
                                            let mut fee_input = String::new();
                                            match std::io::stdin().read_line(&mut fee_input) {
                                                Ok(_) => {
                                                    let fee = fee_input.trim().parse::<i64>();
                                                    match fee {
                                                        Ok(parsed_fee) => {
                                                            // Lógica para realizar la transacción
                                                            if let Err(error) = wallet.make_transaction_index(index, address_receiver, parsed_amount, parsed_fee) {
                                                                println!("Error al realizar la transacción: {}", error);
                                                            } else {
                                                                println!("Transacción realizada correctamente.");
                                                            }
                                                        }
                                                        Err(error) => {
                                                            println!("Error al leer la entrada: {}", error);
                                                        }
                                                    }
                                                }
                                                Err(error) => {
                                                    println!("Error al leer la entrada: {}", error);
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            println!("Error al leer la entrada: {}", error);
                                        }
                                    }
                                }
                                Err(error) => {
                                    println!("Error al leer la entrada: {}", error);
                                }
                            }
                        }
                        Err(error) => {
                            println!("Error al leer la entrada: {}", error);
                        }
                    }
                }
                Err(error) => {
                    println!("Error al leer la entrada: {}", error);
                }
            }
        }
        Err(error) => {
            println!("Error al leer la entrada: {}", error);
        }
    }
                
            
}


fn handle_add_account_request(wallet: &mut Wallet)  {
    println!("Ingrese PRIVATE KEY en formato WIF: ");
    let mut private_key_input = String::new();
    match std::io::stdin().read_line(&mut private_key_input) {
        Ok(_) => {
            let wif_private_key = private_key_input.trim();
            println!("Ingrese la ADDRESS de la cuenta: ");
            let mut address_input = String::new();
            match std::io::stdin().read_line(&mut address_input) {
                Ok(_) => {
                    let address = address_input.trim();
                    println!("Agregando la cuenta -- {} -- a la wallet...\n", address);
                    if let Err(err) = wallet.add_account(wif_private_key.to_string(), address.to_string()) {
                        println!("ERROR: {err}\n");
                        println!("Ocurrio un error al intentar añadir una nueva cuenta, intente de nuevo! \n");
                    } else {
                        println!("Cuenta -- {} -- añadida correctamente a la wallet!\n", address);
                    }
                }
                Err(error) => {
                    println!("Error al leer la entrada: {}", error);
                }
            }
        }
        Err(error) => {
            println!("Error al leer la entrada: {}", error);
        }
    }

}



fn handle_balance_request(wallet: &mut Wallet) {
    println!("Calculando el balance de las cuentas...");
    wallet.show_accounts_balance();
}
#[cfg(test)]
mod tests {

    #[test]
    fn test_archivo_configuracion() {}
}
