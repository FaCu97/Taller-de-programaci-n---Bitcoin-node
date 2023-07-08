use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc, RwLock},
};

use crate::{
    account::Account,
    blocks::{block::Block, block_header::BlockHeader},
    compact_size_uint::CompactSizeUint,
    logwriter::log_writer::{write_in_log, LogSender},
    messages::{
        block_message::{get_block_message, BlockMessage},
        get_data_message::GetDataMessage,
        headers_message::HeadersMessage,
        inventory::Inventory,
        message_header::{get_checksum, HeaderMessage},
        payload::get_data_payload::unmarshalling,
    },
    transactions::transaction::Transaction,
    utxo_tuple::UtxoTuple,
};

use crate::custom_errors::NodeCustomErrors;

type NodeMessageHandlerResult = Result<(), NodeCustomErrors>;
type NodeSender = Sender<Vec<u8>>;

const START_STRING: [u8; 4] = [0x0b, 0x11, 0x09, 0x07];

/*
***************************************************************************
****************************** HANDLERS ***********************************
***************************************************************************
*/

/// Deserializa el payload del mensaje headers y en caso de ser validos se fijan si no estan incluidos en la cadena de headers. En caso
/// de no estarlo, manda por el channel que escribe en el nodo el mensaje getData con el bloque a pedir
pub fn handle_headers_message(
    log_sender: LogSender,
    tx: NodeSender,
    payload: &[u8],
    headers: Arc<RwLock<Vec<BlockHeader>>>,
) -> NodeMessageHandlerResult {
    let new_headers = HeadersMessage::unmarshalling(&payload.to_vec())
        .map_err(|err| NodeCustomErrors::UnmarshallingError(err.to_string()))?;
    for header in new_headers {
        if !header.validate() {
            write_in_log(
                log_sender.error_log_sender.clone(),
                "Error en validacion de la proof of work de nuevo header",
            );
        } else {
            // se fija que el header que recibio no este ya incluido en la cadena de headers (con verificar los ultimos 10 alcanza)
            let header_not_included = header_is_not_included(header, headers.clone())?;
            if header_not_included {
                let get_data_message =
                    GetDataMessage::new(vec![Inventory::new_block(header.hash())]);
                let get_data_message_bytes = get_data_message.marshalling();
                tx.send(get_data_message_bytes)
                    .map_err(|err| NodeCustomErrors::ThreadChannelError(err.to_string()))?;
            }
        }
    }
    Ok(())
}

/// Recibe un Sender de bytes, el payload del mensaje getdata recibido y un vector de cuentas de la wallet y deserializa el mensaje getdata que llega
/// y por cada Inventory que pide si esta como pending_transaction en alguna de las cuentas de la wallet se le envia el mensaje tx con la transaccion pedida
/// por el channel para ser escrita. Devuelve Ok(()) en caso exitoso o error de tipo NodeCustomErrors en caso contrarui
pub fn handle_getdata_message(
    log_sender: LogSender,
    node_sender: NodeSender,
    payload: &[u8],
    blocks: Arc<RwLock<HashMap<[u8; 32], Block>>>,
    accounts: Arc<RwLock<Arc<RwLock<Vec<Account>>>>>,
) -> Result<(), NodeCustomErrors> {
    // idea: mover a GetDataPayload, que devuelva una lista de inventories
    let mut message_to_send: Vec<u8> = Vec::new();

    let inventories = unmarshalling(payload)
        .map_err(|err| NodeCustomErrors::UnmarshallingError(err.to_string()))?;

    for inv in inventories {
        //  MSG_TX == 1
        if inv.type_identifier == 1 {
            for account in &*accounts
                .read()
                .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
                .read()
                .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
            {
                for tx in &*account
                    .pending_transactions
                    .read()
                    .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
                {
                    if tx.hash() == inv.hash {
                        // mover get_tx_message a otro módulo?
                        let tx_message = get_tx_message(tx);
                        node_sender
                            .send(tx_message)
                            .map_err(|err| NodeCustomErrors::ThreadChannelError(err.to_string()))?;
                        write_in_log(
                            log_sender.clone().info_log_sender,
                            format!("transaccion {:?} enviada", tx.hex_hash()).as_str(),
                        );
                    }
                }
            }
        }
        //  MSG_BLOCK == 2
        if inv.type_identifier == 2 {
            let block_hash = inv.hash;
            // buscar el bloque en la blockchain
            match blocks
                .read()
                .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
                .get(&block_hash)
            {
                Some(block) => {
                    message_to_send.extend_from_slice(&get_block_message(block));
                }
                None => {
                    // enviar mensaje notfound

                    write_in_log(
                        log_sender.error_log_sender.clone(),
                        &format!(
                            "No se encontro el bloque en la blockchain: {}",
                            crate::account::bytes_to_hex_string(&inv.hash)
                        ),
                    );
                }
            }
        }
    }
    node_sender
        .send(message_to_send)
        .map_err(|err| NodeCustomErrors::ThreadChannelError(err.to_string()))?;
    Ok(())
}

// Devuelve el mensaje tx según la transacción recibida
fn get_tx_message(tx: &Transaction) -> Vec<u8> {
    let mut tx_payload = vec![];
    tx.marshalling(&mut tx_payload);
    let header = HeaderMessage::new("tx".to_string(), Some(&tx_payload));
    let mut tx_message = vec![];
    tx_message.extend_from_slice(&header.to_le_bytes());
    tx_message.extend_from_slice(&tx_payload);

    tx_message
}

/// Deserializa el payload del mensaje blocks y en caso de que el bloque es valido y todavia no este incluido, agrega el header a la cadena de headers
/// y el bloque a la cadena de bloques. Se fija si alguna transaccion del bloque involucra a alguna de las cuentas del programa.
pub fn handle_block_message(
    log_sender: LogSender,
    payload: &[u8],
    headers: Arc<RwLock<Vec<BlockHeader>>>,
    blocks: Arc<RwLock<HashMap<[u8; 32], Block>>>,
    accounts: Arc<RwLock<Arc<RwLock<Vec<Account>>>>>,
    utxo_set: Arc<RwLock<HashMap<[u8; 32], UtxoTuple>>>,
) -> NodeMessageHandlerResult {
    let new_block = BlockMessage::unmarshalling(&payload.to_vec())
        .map_err(|err| NodeCustomErrors::UnmarshallingError(err.to_string()))?;
    if new_block.validate().0 {
        let header_is_not_included_yet =
            header_is_not_included(new_block.block_header, headers.clone())?;
        if header_is_not_included_yet {
            include_new_header(log_sender.clone(), new_block.block_header, headers)?;
            include_new_block(log_sender.clone(), new_block.clone(), blocks)?;
            new_block.contains_pending_tx(log_sender, accounts.clone())?;
            new_block
                .give_me_utxos(utxo_set.clone())
                .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?;
            update_accounts_utxo_set(accounts, utxo_set)?;
        }
    } else {
        write_in_log(
            log_sender.error_log_sender,
            "NUEVO BLOQUE ES INVALIDO, NO LO AGREGO!",
        );
    }
    Ok(())
}

/// Recieves a NodeSender and the payload of the inv message and creates the inventories to ask for the incoming
/// txs the node sent via inv. Returns error in case of failure or Ok(())
pub fn handle_inv_message(
    tx: NodeSender,
    payload: &[u8],
    transactions_received: Arc<RwLock<Vec<[u8; 32]>>>,
) -> NodeMessageHandlerResult {
    let mut offset: usize = 0;
    let count = CompactSizeUint::unmarshalling(payload, &mut offset)
        .map_err(|err| NodeCustomErrors::UnmarshallingError(err.to_string()))?;
    let mut inventories = vec![];
    for _ in 0..count.decoded_value() as usize {
        let mut inventory_bytes = vec![0; 36];
        inventory_bytes.copy_from_slice(&payload[offset..(offset + 36)]);
        let inv = Inventory::from_le_bytes(&inventory_bytes);
        if inv.type_identifier == 1
            && !transactions_received
                .read()
                .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
                .contains(&inv.hash())
        {
            transactions_received
                .write()
                .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
                .push(inv.hash());
            inventories.push(inv);
        }
        offset += 36;
    }
    if !inventories.is_empty() {
        ask_for_incoming_tx(tx, inventories)?;
    }
    Ok(())
}

/// Recibe un NodeSender y un payload y manda por el channel el pong message correspondiente para que se escriba por el nodo
/// y quede respondido el ping. Devuelve Ok(()) en caso de que se pueda enviar bien por el channel o Error de channel en caso contrario.
pub fn handle_ping_message(tx: NodeSender, payload: &[u8]) -> NodeMessageHandlerResult {
    let header = HeaderMessage {
        start_string: START_STRING,
        command_name: "pong".to_string(),
        payload_size: payload.len() as u32,
        checksum: get_checksum(payload),
    };
    let header_bytes = HeaderMessage::to_le_bytes(&header);
    let mut message: Vec<u8> = Vec::new();
    message.extend_from_slice(&header_bytes);
    message.extend(payload);
    tx.send(message)
        .map_err(|err| NodeCustomErrors::ThreadChannelError(err.to_string()))?;
    Ok(())
}

/// Recibe un LogSender, el Payload del mensaje tx y un puntero a un puntero con las cuentas de la wallet. Se fija si la tx involucra una cuenta de nuestra wallet. Devuelve Ok(())
/// en caso de que se pueda leer bien el payload y recorrer las tx o error en caso contrario
pub fn handle_tx_message(
    log_sender: LogSender,
    payload: &[u8],
    accounts: Arc<RwLock<Arc<RwLock<Vec<Account>>>>>,
) -> NodeMessageHandlerResult {
    let tx = Transaction::unmarshalling(&payload.to_vec(), &mut 0)
        .map_err(|err| NodeCustomErrors::UnmarshallingError(err.to_string()))?;
    tx.check_if_tx_involves_user_account(log_sender, accounts)?;
    Ok(())
}

/*
***************************************************************************
***************************************************************************
***************************************************************************
*/

/// Receives the inventories with the tx and the sender to write in the node. Sends the getdata message to ask for the tx
fn ask_for_incoming_tx(tx: NodeSender, inventories: Vec<Inventory>) -> NodeMessageHandlerResult {
    let get_data_message = GetDataMessage::new(inventories);
    let get_data_message_bytes = get_data_message.marshalling();
    tx.send(get_data_message_bytes)
        .map_err(|err| NodeCustomErrors::ThreadChannelError(err.to_string()))?;
    Ok(())
}

/// Recibe un bloque a agregar a la cadena y el puntero Arc apuntando a la cadena de bloques y lo agrega.
/// Devuelve Ok(()) en caso de poder agregarlo correctamente o error del tipo NodeHandlerError en caso de no poder.
fn include_new_block(
    log_sender: LogSender,
    block: Block,
    blocks: Arc<RwLock<HashMap<[u8; 32], Block>>>,
) -> NodeMessageHandlerResult {
    println!("\nRECIBO NUEVO BLOQUE: {} \n", block.hex_hash());
    write_in_log(
        log_sender.info_log_sender,
        format!("NUEVO BLOQUE AGREGADO: -- {} --", block.hex_hash()).as_str(),
    );
    blocks
        .write()
        .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
        .insert(block.hash(), block);
    Ok(())
}

/// Recibe un header a agregar a la cadena de headers y el Arc apuntando a la cadena de headers y lo agrega
/// Devuelve Ok(()) en caso de poder agregarlo correctamente o error del tipo NodeHandlerError en caso de no poder
fn include_new_header(
    log_sender: LogSender,
    header: BlockHeader,
    headers: Arc<RwLock<Vec<BlockHeader>>>,
) -> NodeMessageHandlerResult {
    headers
        .write()
        .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?
        .push(header);
    write_in_log(
        log_sender.info_log_sender,
        "Recibo un nuevo header, lo agrego a la cadena de headers!",
    );
    Ok(())
}

/// Recibe un header y la lista de headers y se fija en los ulitmos 10 headers de la lista, si es que existen, que el header
/// no este incluido ya. En caso de estar incluido devuelve false y en caso de nos estar incluido devuelve true. Devuelve error en caso de
/// que no se pueda leer la lista de headers
fn header_is_not_included(
    header: BlockHeader,
    headers: Arc<RwLock<Vec<BlockHeader>>>,
) -> Result<bool, NodeCustomErrors> {
    let headers_guard = headers
        .read()
        .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?;
    let start_index = headers_guard.len().saturating_sub(10);
    let last_10_headers = &headers_guard[start_index..];
    // Verificar si el header está en los ultimos 10 headers
    for included_header in last_10_headers.iter() {
        if *included_header == header {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Actualiza el utxo_set de cada cuenta
fn update_accounts_utxo_set(
    accounts: Arc<RwLock<Arc<RwLock<Vec<Account>>>>>,
    utxo_set: Arc<RwLock<HashMap<[u8; 32], UtxoTuple>>>,
) -> Result<(), NodeCustomErrors> {
    let accounts_lock = accounts
        .read()
        .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?;
    let mut accounts_inner_lock = accounts_lock
        .write()
        .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?;

    for account_lock in accounts_inner_lock.iter_mut() {
        account_lock
            .set_utxos(utxo_set.clone())
            .map_err(|err| NodeCustomErrors::LockError(err.to_string()))?;
    }
    Ok(())
}
