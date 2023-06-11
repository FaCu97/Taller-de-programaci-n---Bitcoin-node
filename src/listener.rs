use std::{
    error::Error,
    io::Read,
    net::TcpStream,
    sync::{Arc, RwLock},
};

use crate::{
    blocks::block_header::BlockHeader,
    compact_size_uint::CompactSizeUint,
    logwriter::log_writer::{write_in_log, LogSender},
    messages::{
        get_data_message::GetDataMessage,
        headers_message::{is_terminated, HeadersMessage},
        inventory::Inventory,
        message_header::{write_pong_message, HeaderMessage},
    }, transactions::transaction::Transaction, wallet::Wallet, account::Account
};

/// Recives a node to listen from and a pointer to a bool to stop the cycle of listening in case this is false. Reads
/// header-payload until it founds a header representing an incoming headers message. In that case returns a Vec<BlockHeader>
/// which contains the headers recieved from the node. In case that the message is not "headers" checks if it is a handleable
/// message (ping, inv, tx) and handles it depending of the message.
pub fn listen_for_incoming_messages(
    log_sender: LogSender,
    wallet: Wallet,
    transactions_reccieved: Arc<RwLock<Vec<[u8; 32]>>>,
    pending_transactions: Arc<RwLock<Vec<Transaction>>>,
    stream: &mut TcpStream,
    finish: Option<Arc<RwLock<bool>>>,
) -> Result<Vec<BlockHeader>, Box<dyn std::error::Error>> {
    let mut buffer_num = [0; 24];
    stream.read_exact(&mut buffer_num)?;
    let mut header = HeaderMessage::from_le_bytes(buffer_num)?;
    while !header.command_name.contains("headers") && !is_terminated(finish.clone()) {
        let payload_size = header.payload_size as usize;
        let mut payload_buffer_num: Vec<u8> = vec![0; payload_size];
        stream.read_exact(&mut payload_buffer_num)?;
        match &header.command_name {
            header_name if header_name.contains("inv") => {
                let node = stream.try_clone()?;
                if let Err(err) = handle_inv_message(node, payload_buffer_num, transactions_reccieved.clone()) {
                    write_in_log(
                        log_sender.error_log_sender.clone(),
                        format!(
                            "Error {} al recibir transaccion del nodo {:?}",
                            err,
                            stream.peer_addr()?
                        )
                        .as_str(),
                    );
                };
            }
            header_name if header_name.contains("ping") => {
                write_in_log(
                    log_sender.messege_log_sender.clone(),
                    format!(
                        "Recibo Correctamente: ping -- Nodo: {:?}",
                        stream.peer_addr()?
                    )
                    .as_str(),
                );
                let mut node = stream.try_clone()?;
                write_pong_message(&mut node, &payload_buffer_num)?;
            }
            header_name if header_name.contains("tx") => {
                write_in_log(
                    log_sender.messege_log_sender.clone(),
                    format!(
                        "Recibo Correctamente: tx -- Nodo: {:?}",
                        stream.peer_addr()?
                    )
                    .as_str(),
                );
                let tx = Transaction::unmarshalling(&payload_buffer_num, &mut 0)?;
                //println!("TX:    {:?}\n", tx);

                check_if_tx_involves_user_account(tx, wallet.accounts.clone(), pending_transactions.clone());
            }
            _ => {
                write_in_log(
                    log_sender.messege_log_sender.clone(),
                    format!(
                        "IGNORADO -- Recibo: {} -- Nodo: {:?}",
                        header.command_name,
                        stream.peer_addr()?
                    )
                    .as_str(),
                );
            }
        }
        buffer_num = [0; 24];
        stream.read_exact(&mut buffer_num)?;
        header = HeaderMessage::from_le_bytes(buffer_num)?;
    }
    if !is_terminated(finish) {
        let payload_size = header.payload_size as usize;
        let mut payload_buffer_num: Vec<u8> = vec![0; payload_size];
        stream.read_exact(&mut payload_buffer_num)?;
        let new_headers = HeadersMessage::unmarshalling(&payload_buffer_num)?;
        Ok(new_headers)
    } else {
        Err("no llegaron nuevos headers!".into())
    }
}

/// recieves a Node and the payload of the inv message and creates the invetories to ask for the incoming
/// txs the node sent via inv. Returns error in case of failure or Ok(())
fn handle_inv_message(stream: TcpStream, payload_bytes: Vec<u8>, transactions_reccieved: Arc<RwLock<Vec<[u8; 32]>>>) -> Result<(), Box<dyn Error>> {
    let mut offset: usize = 0;
    let count = CompactSizeUint::unmarshalling(&payload_bytes, &mut offset)?;
    let mut inventories = vec![];
    for _ in 0..count.decoded_value() as usize {
        let mut inventory_bytes = vec![0; 36];
        inventory_bytes.copy_from_slice(&payload_bytes[offset..(offset + 36)]);
        let inv = Inventory::from_le_bytes(&inventory_bytes);
        if inv.type_identifier == 1{
            if !transactions_reccieved.read().unwrap().contains(&inv.hash()){
                transactions_reccieved.write().unwrap().push(inv.hash());
                inventories.push(inv);
            }
        }
        offset += 36;
    }
    if inventories.len() > 0 {
        ask_for_incoming_tx(stream, inventories).map_err(Box::new)?;
    }
    Ok(())
}

/// Recieves the invetories with the tx and the node. Writes the getdata message to ask for the tx
fn ask_for_incoming_tx(
    mut stream: TcpStream,
    inventories: Vec<Inventory>,
) -> Result<(), std::io::Error> {
    let get_data_message = GetDataMessage::new(inventories);
    get_data_message.write_to(&mut stream)?;
    Ok(())
}


fn check_if_tx_involves_user_account(tx: Transaction, accounts: Vec<Account>, pending_transactions: Arc<RwLock<Vec<Transaction>>>) {
    for tx_out in tx.tx_out.clone() {
        tx_out.involves_user_account(accounts.clone(), tx.clone(), pending_transactions.clone());
    }
}
