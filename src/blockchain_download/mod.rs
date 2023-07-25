use gtk::glib;

use self::blocks_download::{download_blocks, download_blocks_single_node};
use self::headers_download::{download_missing_headers, get_initial_headers};
use self::utils::{get_amount_of_headers_and_blocks, get_node, join_threads, return_node_to_vec};
use super::blocks::block::Block;
use super::blocks::block_header::BlockHeader;
use super::config::Config;
use super::logwriter::log_writer::{write_in_log, LogSender};
use crate::custom_errors::NodeCustomErrors;
use crate::gtk::ui_events::UIEvent;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::mpsc::channel;
use std::sync::{Arc, RwLock};
use std::{thread, vec};
mod blocks_download;
pub(crate) mod headers_download;
mod utils;

// Gensis block header hardcoded to start the download (this is the first block of the blockchain)
// data taken from: https://en.bitcoin.it/wiki/Genesis_block
const GENESIS_BLOCK_HEADER: BlockHeader = BlockHeader {
    version: 1,
    previous_block_header_hash: [0; 32],
    merkle_root_hash: [
        59, 163, 237, 253, 122, 123, 18, 178, 122, 199, 44, 62, 103, 118, 143, 97, 127, 200, 27,
        195, 136, 138, 81, 50, 58, 159, 184, 170, 75, 30, 94, 74,
    ],
    time: 1296677802,
    n_bits: 486604799,
    nonce: 414098458,
};

type HeadersBlocksTuple = (
    Arc<RwLock<Vec<BlockHeader>>>,
    Arc<RwLock<HashMap<[u8; 32], Block>>>,
    Arc<RwLock<HashMap<[u8; 32], usize>>>,
);

/// Recieves a list of TcpStreams that are the connection with nodes already established and downloads
/// all the headers from the blockchain and the blocks from a config date. Returns the headers and blocks in
/// two separete lists in case of exit or an error in case of faliure
pub fn initial_block_download(
    config: &Arc<Config>,
    log_sender: &LogSender,
    ui_sender: &Option<glib::Sender<UIEvent>>,
    nodes: Arc<RwLock<Vec<TcpStream>>>,
) -> Result<HeadersBlocksTuple, NodeCustomErrors> {
    write_in_log(
        &log_sender.info_log_sender,
        "EMPIEZA DESCARGA INICIAL DE BLOQUES",
    );
    // el vector de headers empieza con el header del bloque genesis
    let headers = vec![GENESIS_BLOCK_HEADER];
    let pointer_to_headers = Arc::new(RwLock::new(headers));
    let blocks: HashMap<[u8; 32], Block> = HashMap::new();
    let pointer_to_blocks = Arc::new(RwLock::new(blocks));
    let mut heights_hashmap: HashMap<[u8; 32], usize> = HashMap::new();
    heights_hashmap.insert([0u8; 32], 0); // genesis hash
    let header_heights: Arc<RwLock<HashMap<[u8; 32], usize>>> =
        Arc::new(RwLock::new(heights_hashmap));
    get_initial_headers(
        config,
        log_sender,
        ui_sender,
        pointer_to_headers.clone(),
        header_heights.clone(),
        nodes.clone(),
    )?;
    let amount_of_nodes = nodes
        .read()
        .map_err(|err| NodeCustomErrors::LockError(format!("{:?}", err)))?
        .len();
    if config.ibd_single_node || amount_of_nodes < 2 {
        download_full_blockchain_from_single_node(
            config,
            log_sender,
            ui_sender,
            nodes,
            pointer_to_headers.clone(),
            pointer_to_blocks.clone(),
            header_heights.clone(),
        )?;
    } else {
        download_full_blockchain_from_multiple_nodes(
            config,
            log_sender,
            ui_sender,
            nodes,
            pointer_to_headers.clone(),
            pointer_to_blocks.clone(),
            header_heights.clone(),
        )?;
    }
    let (amount_of_headers, amount_of_blocks) =
        get_amount_of_headers_and_blocks(&pointer_to_headers, &pointer_to_blocks)?;
    write_in_log(
        &log_sender.info_log_sender,
        format!("TOTAL DE HEADERS DESCARGADOS: {}", amount_of_headers).as_str(),
    );
    write_in_log(
        &log_sender.info_log_sender,
        format!("TOTAL DE BLOQUES DESCARGADOS: {}\n", amount_of_blocks).as_str(),
    );
    Ok((pointer_to_headers, pointer_to_blocks, header_heights))
}

/// Se encarga de descargar todos los headers y bloques de la blockchain en multiples thread, en un thread descarga los headers
/// y en el otro a medida que se van descargando los headers va pidiendo los bloques correspondientes.
/// Devuelve error en caso de falla.
fn download_full_blockchain_from_multiple_nodes(
    config: &Arc<Config>,
    log_sender: &LogSender,
    ui_sender: &Option<glib::Sender<UIEvent>>,
    nodes: Arc<RwLock<Vec<TcpStream>>>,
    headers: Arc<RwLock<Vec<BlockHeader>>>,
    blocks: Arc<RwLock<HashMap<[u8; 32], Block>>>,
    header_heights: Arc<RwLock<HashMap<[u8; 32], usize>>>,
) -> Result<(), NodeCustomErrors> {
    // channel to comunicate headers download thread with blocks download thread
    let (tx, rx) = channel();
    let mut threads_handle = vec![];
    let config_cloned = config.clone();
    let log_sender_cloned = log_sender.clone();
    let nodes_cloned = nodes.clone();
    let headers_cloned = headers.clone();
    let tx_cloned = tx.clone();
    let ui_sender_clone = ui_sender.clone();
    threads_handle.push(thread::spawn(move || {
        download_missing_headers(
            &config_cloned,
            &log_sender_cloned,
            &ui_sender_clone,
            nodes_cloned,
            headers_cloned,
            header_heights,
            tx_cloned,
        )
    }));
    let config = config.clone();
    let log_sender = log_sender.clone();
    let ui_sender = ui_sender.clone();
    threads_handle.push(thread::spawn(move || {
        download_blocks(
            &config,
            &log_sender,
            &ui_sender,
            nodes,
            blocks,
            headers,
            rx,
            tx,
        )
    }));
    join_threads(threads_handle)?;
    Ok(())
}

/// Se encarga de descargar todos los headers y bloques de la blockchain en un solo thread, primero descarga todos los headers
/// y luego descarga todos los bloques. Devuelve error en caso de falla.
fn download_full_blockchain_from_single_node(
    config: &Arc<Config>,
    log_sender: &LogSender,
    ui_sender: &Option<glib::Sender<UIEvent>>,
    nodes: Arc<RwLock<Vec<TcpStream>>>,
    headers: Arc<RwLock<Vec<BlockHeader>>>,
    blocks: Arc<RwLock<HashMap<[u8; 32], Block>>>,
    header_heights: Arc<RwLock<HashMap<[u8; 32], usize>>>,
) -> Result<(), NodeCustomErrors> {
    let (tx, rx) = channel();
    download_missing_headers(
        config,
        log_sender,
        ui_sender,
        nodes.clone(),
        headers,
        header_heights,
        tx,
    )?;
    let mut node = get_node(nodes.clone())?;
    for blocks_to_download in rx {
        download_blocks_single_node(
            config,
            log_sender,
            ui_sender,
            blocks_to_download,
            &mut node,
            blocks.clone(),
        )?;
    }
    return_node_to_vec(nodes, node)?;
    Ok(())
}

/*
/// Once the headers are downloaded, this function recieves the nodes and headers  downloaded
/// and sends a getheaders message to each node to compare and get a header that was not downloaded.
/// it returns error in case of failure.
fn compare_and_ask_for_last_headers(
    config: &Arc<Config>,
    log_sender: &LogSender,
    nodes: Arc<RwLock<Vec<TcpStream>>>,
    headers: Arc<RwLock<Vec<BlockHeader>>>,
) -> Result<Vec<BlockHeader>, NodeCustomErrors> {
    // voy guardando los nodos que saco aca para despues agregarlos al puntero
    let mut nodes_vec: Vec<TcpStream> = vec![];
    let mut new_headers = vec![];
    // recorro todos los nodos
    while !nodes
        .read()
        .map_err(|err| NodeCustomErrors::LockError(format!("{:?}", err)))?
        .is_empty()
    {
        let mut node = nodes
            .write()
            .map_err(|err| NodeCustomErrors::LockError(format!("{:?}", err)))?
            .pop()
            .ok_or("Error no hay mas nodos para comparar y descargar ultimos headers!\n")
            .map_err(|err| NodeCustomErrors::CanNotRead(err.to_string()))?;
        let last_header = headers
            .read()
            .map_err(|err| NodeCustomErrors::LockError(format!("{:?}", err)))?
            .last()
            .ok_or("Error no hay headers guardados, no tengo para comparar...\n")
            .map_err(|err| NodeCustomErrors::CanNotRead(err.to_string()))?
            .hash();
        GetHeadersMessage::build_getheaders_message(config, vec![last_header])
            .write_to(&mut node)
            .map_err(|err| NodeCustomErrors::WriteNodeError(err.to_string()))?;
        let headers_read = match HeadersMessage::read_from(log_sender, &mut node, None) {
            Ok(headers) => headers,
            Err(err) => {
                write_in_log(
                    &log_sender.error_log_sender,
                    format!("Error al tratar de leer nuevos headers, descarto nodo. Error: {err}")
                        .as_str(),
                );
                continue;
            }
        };
        // si se recibio un header nuevo lo agrego
        if !headers_read.is_empty() {
            headers
                .write()
                .map_err(|err| NodeCustomErrors::LockError(format!("{:?}", err)))?
                .extend_from_slice(&headers_read);
            write_in_log(
                &log_sender.info_log_sender,
                format!(
                    "{} headers encontrados al comparar el ultimo mio con el nodo: {:?}",
                    headers_read.len(),
                    node
                )
                .as_str(),
            );
            new_headers.extend_from_slice(&headers_read);
        }
        nodes_vec.push(node);
    }
    // devuelvo todos los nodos a su puntero
    nodes
        .write()
        .map_err(|err| NodeCustomErrors::LockError(format!("{:?}", err)))?
        .extend(nodes_vec);
    Ok(new_headers)
}
*/
