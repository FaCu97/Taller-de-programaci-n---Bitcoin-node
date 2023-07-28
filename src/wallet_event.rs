type Address = String;
type WifPrivateKey = String;
type AccountIndex = usize;
type Amount = i64;
type Fee = i64;
type BlockHash = String;
type TransactionHash = String;

pub enum WalletEvent {
    Start,
    AddAccountRequest(WifPrivateKey, Address),
    MakeTransactionRequest(Address, Amount, Fee),
    PoiOfTransactionRequest(BlockHash, TransactionHash),
    Finish,
    ChangeAccount(AccountIndex),
}