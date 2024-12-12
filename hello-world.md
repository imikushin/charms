```sh
alias b=bitcoin-cli
```

```sh
recipient="$(b getnewaddress)"

rawtxhex=$(b createrawtransaction '''[]''' '''[{ "'$recipient'": 0.00010000 }]''')

# or something like this if you want to spend an existing Charm
# rawtxhex=$(b createrawtransaction '''[ { "txid": "dafd94568e0d8fb0e72c9bb84e54b227c9cad28168611fe3d37f06276125e247", "vout": 0 } ]''' '''[{ "'$recipient'": 0.00010000 }]''')

fee_rate=2

draft_tx_hex=$(b -named fundrawtransaction $rawtxhex changePosition=1 fee_rate=$fee_rate | jq -r '.hex')

# now choose a funding output with a few thousand sats (50000 should be more than enough)
b listunspent

# MUST NOT be one of those being spent by $draft_tx_hex (printed out by this)
b decoderawtransaction $(echo $draft_tx_hex) | jq -r '.vin[] | "\(.txid):\(.vout)"'

funding_utxo_id=acbef6b2f3808ad4fe36fff4d70ba1d0ccc05ce254d8096a8591de76683af8d0:0
funding_utxo_value=10000
# value in sats

change_address=$(b getrawchangeaddress)

b decoderawtransaction $draft_tx_hex

# now get the hex representation of $draft_tx_hex's input transactions

prev_txs=$(b decoderawtransaction $draft_tx_hex | jq -r '.vin[].txid' | sort | uniq | xargs -I {} bitcoin-cli getrawtransaction {} | paste -sd, -)

RUST_LOG=info charms spell prove --spell=./tmp/toad-with-nft/spell-mint-nft.yml --tx=$draft_tx_hex --prev-txs $prev_txs --app-bins examples/toad-token/elf/riscv32im-succinct-zkvm-elf --funding-utxo-id=$funding_utxo_id --funding-utxo-value=$funding_utxo_value --change-address=$change_address --fee-rate=$fee_rate

# sign the resulting transactions

spell_prove_result='["0200000001d0...000","020000000001041...000"]'

signed_commit_tx=$(b signrawtransactionwithwallet $(echo $spell_prove_result | jq -r '.[0]') | jq -r '.hex')

signed_spell_tx=$(b signrawtransactionwithwallet $(echo $spell_prove_result | jq -r '.[1]') $(b decoderawtransaction $signed_commit_tx | jq -c '[{txid: .txid, vout: .vout[0].n, scriptPubKey: .vout[0].scriptPubKey.hex, amount: .vout[0].value}]') | jq -r '.hex')

b submitpackage '["'$signed_commit_tx'","'$signed_spell_tx'"]'
```