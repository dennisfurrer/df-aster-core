# Conformance vectors from Aster's own reference stack: eth_account, the library Aster's
# official Python demo (asterdex/api-docs demo/aster-code.py) signs with. The signing
# functions below are the demo's `sign_v3` and `sign_v3_eip712`, with the deprecated
# encode_structured_data replaced by encode_typed_data. Keys are derived from labels
# (keccak256 of a string), never a real wallet; the agent key is the one Aster's API docs
# publish as an example. Nothing leaves the machine.
#
# Run from this directory, writing ../aster_python.json:
#   uv run --with eth-account==0.13.7 python gen.py > ../aster_python.json
import json
from copy import deepcopy

from eth_account import Account
from eth_account.messages import encode_typed_data
from eth_utils import keccak

MAIN_KEY = "0x" + keccak(text="atlas aster main").hex()
DOCS_AGENT_KEY = "0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1"
main = Account.from_key(MAIN_KEY)
agent = Account.from_key(DOCS_AGENT_KEY)

DOMAIN_TYPES = [
    {"name": "name", "type": "string"},
    {"name": "version", "type": "string"},
    {"name": "chainId", "type": "uint256"},
    {"name": "verifyingContract", "type": "address"},
]


def sign_v3(private_key, message, chain_id):
    typed = {
        "types": {"EIP712Domain": DOMAIN_TYPES, "Message": [{"name": "msg", "type": "string"}]},
        "primaryType": "Message",
        "domain": {
            "name": "AsterSignTransaction",
            "version": "1",
            "chainId": chain_id,
            "verifyingContract": "0x0000000000000000000000000000000000000000",
        },
        "message": {"msg": message},
    }
    encoded = encode_typed_data(full_message=typed)
    signed = Account.sign_message(encoded, private_key=private_key)
    return typed, "0x" + signed.message_hash.hex().removeprefix("0x"), "0x" + signed.signature.hex().removeprefix("0x")


def infer(value):
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "uint256"
    return "string"


def sign_v3_eip712(private_key, params, primary_type):
    values = {k[:1].upper() + k[1:]: v for k, v in params.items()}
    typed = {
        "types": {"EIP712Domain": DOMAIN_TYPES, primary_type: [{"name": k, "type": infer(v)} for k, v in values.items()]},
        "primaryType": primary_type,
        "domain": {
            "name": "AsterSignTransaction",
            "version": "1",
            "chainId": 56,
            "verifyingContract": "0x0000000000000000000000000000000000000000",
        },
        "message": values,
    }
    encoded = encode_typed_data(full_message=deepcopy(typed))
    signed = Account.sign_message(encoded, private_key=private_key)
    return typed, "0x" + signed.message_hash.hex().removeprefix("0x"), "0x" + signed.signature.hex().removeprefix("0x")


NONCE = 1790000000000000
messages = [
    ("docs_order", "symbol=ASTERUSDT&type=LIMIT&side=BUY&timeInForce=GTC&quantity=20&price=0.5&nonce=1790000000000000&signer=" + agent.address),
    ("encoded_batch", "batchOrders=%5B%7B%22symbol%22%3A%22BTCUSDT%22%7D%5D&nonce=1790000000000001&user=" + main.address + "&signer=" + agent.address),
    ("cjk", "symbol=%E9%BE%99%E8%99%BEUSDT&nonce=1790000000000002&user=" + main.address + "&signer=" + agent.address),
    ("empty_params", "nonce=1790000000000003&user=" + main.address + "&signer=" + agent.address),
]
message_cases = []
for name, msg in messages:
    typed, digest, signature = sign_v3(DOCS_AGENT_KEY, msg, 1666)
    message_cases.append({"name": name, "msg": msg, "chainId": 1666, "signingHash": digest, "signature": signature})

# registerAndApproveAgent as asterdex/API-demo signs it: ipWhitelist only when there is one,
# signed under chain id 1666 while the request names signatureChainId=56; and once under 56, as
# the docs describe.
register = []
for name, ips, chain in [("demo", "", 1666), ("docs_with_ips", "1.2.3.4 10.0.0.0/24", 56)]:
    msg = (
        f"user={main.address}&nonce={NONCE}&agentName=bot&agentAddress={agent.address}&expired=1792592000000"
        "&signatureChainId=56&canSpotTrade=false&canPerpTrade=true&canWithdraw=false"
    )
    if ips:
        msg += "&ipWhitelist=" + ips
    _, digest, signature = sign_v3(MAIN_KEY, msg, chain)
    register.append({"name": name, "msg": msg, "ipWhitelist": ips, "chainId": chain, "signingHash": digest, "signature": signature})

actions = []
for primary, params in [
    ("ApproveAgent", {"agentName": "ivanbuilder", "agentAddress": agent.address, "ipWhitelist": "", "expired": 1967945395040,
                      "canSpotTrade": True, "canPerpTrade": False, "canWithdraw": False}),
    ("UpdateAgent", {"agentAddress": agent.address, "ipWhitelist": "101.198.86.182,192.168.1.100",
                     "canSpotTrade": False, "canPerpTrade": True, "canWithdraw": False}),
    ("DelAgent", {"agentAddress": agent.address}),
    ("ApproveBuilder", {"builder": main.address, "maxFeeRate": "0.00001", "builderName": "ivan3"}),
    ("UpdateBuilder", {"builder": main.address, "maxFeeRate": "0.00002"}),
    ("DelBuilder", {"builder": main.address}),
]:
    full = dict(params)
    full["asterChain"] = "Mainnet"
    full["user"] = main.address
    full["nonce"] = NONCE
    typed, digest, signature = sign_v3_eip712(MAIN_KEY, full, primary)
    actions.append({"primaryType": primary, "params": full, "typedData": typed, "signingHash": digest, "signature": signature})

print(json.dumps({
    "source": "eth_account 0.13.7 running the signing functions of asterdex/api-docs demo/aster-code.py",
    "keys": {"main": "keccak256(\"atlas aster main\")", "agent": "the example agent key in Aster's API docs"},
    "mainAddress": main.address,
    "agentAddress": agent.address,
    "messages": message_cases,
    "registerAndApproveAgent": register,
    "mainActions": actions,
}, indent=1))
