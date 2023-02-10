import base64

# HexBytes

from web3 import Web3, HTTPProvider
from eth_account.messages import encode_defunct

w3 = Web3()

message = encode_defunct(text='XMTP : Create Identity\n0880b4eee9a3e48fa1171a430a4104136ed88fc47dede4a85a3130137e54de9078dfd4bd3927ddacfac38c61fdfc94390826fcae314ad6fc7185b1c63366ead6fd0f5dd93a93fb646e15e69f65849b\n\nFor more info: https://xmtp.org/signatures/')
print(message)
# Needed to add 1b to the end of the signature output
signature = "a39f6fe954c0d0a726b31641aa6f63560db4585e6176b9807e6367e2291cfc6b200b56caf30f4cc5bb5a905be0cb1cae27bdc916a9ec7f176f33cf6c9284c9b21b"

#message = encode_defunct(text="I♥SF")
#signature = '0xe6ca9bba58c88611fad66a6ce8f996908195593807c4b38bd528d2cff09d4eb33e5bfbbf4d3e39b1a2fd816a7680c19ebebaf3a141b239934ad43cb33fcec8ce1c'

#message = encode_defunct(text="hello world!")
#signature = "044eb47c187a8053e0b85911533a75868f0830d5cb8ffd75849906f2a2cc093b27ef5fdbdfb53a4019a06c3e14b0875abfc6f5c2867e265fde59fb6d3b2cb889"

# Get HexBytes from signature
import pdb; pdb.set_trace()
# set a breakpoint on _recover_hash

address = w3.eth.account.recover_message(message, signature=signature)
print(address)
