---
id: rlay-ontology-serialization-formats
title: Rlay Ontology - Serialization Formats
sidebar_label: Serialization Formats
---

The `rlay_ontology` crate provides multiple serialization formats for Rlay Ontology Entities:

- Protobuf based format - currently the main format used in the Solidity OntologyStorage, and the one used for calculating the CIDs
- Web3 / JSON based format - Used for the JSONRPC in `rlay-client` and the representation in the Javascript libraries
- v0 CBOR format - CBOR based format for future use. Still under development and not fully specified.


### Protobuf based format

**Pros**:
- Protobuf libaries were easily available for prototyping in Rust and Solidity at time of creation
- Via ordered fields in protobuf schemas, it is pretty easy to have a determenistic content-addressable format
- Low size overhead over contents

**Cons**:
- Protobuf is comparatively complex for the simple features we need of it
- As the protobuf encoding doesn't contain any information about the entity kind, the entity kind has to be known for the encoding to be correctly interpreted
- Per-EntityKind CID multicodecs would require a lot of codecs to be registered/coordinated
- Unwieldy to use in end-user applications

-----

- Used for CID calculation
- See [ontology.proto](https://github.com/rlay-project/rlay-ontology/blob/master/rlay_ontology/src/ontology.proto) for the Protobuf schema
- Before calculating the CID, all the Array fields are sorted, to bring the entity into a determenistic canonicalized format
- Values in non-CID bytes fields is not strictly defined but assumed to be CBOR encoded
- Each [EntityKind](./generated/rlay-ontology-entities) has a different 3 byte CID multicodec (which are not registered in the official multicodec list).
- Uses a `keccak-256` hash of the protobuf encoding of the entity for CID calculation
- Example CID in hex encoding:
```
019580031b2088868a58d3aac6d2558a29b3b8cacf3c9788364f57a3470158283121a15dcae0
^^      ^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
| ^^^^^^ |^^     |
|   |    | |     |
|   |    | |     32 byte keccak-256 hash of protobuf encoded entity
|   |    | |
|   |    | 1 byte variint specifying a multihash length of 32
|   |    |
|   |    Multihash identifier for a keccak-256 hash
|   |
|   3 byte multicodec for entity kind (in this case Annotation)
|
1 byte varint for CID version
```

### Web3 / JSON based format

**Pros**:
- Easy to use in end user applications, where entities are read/created/modified
- `0x` encoding for byte fields fits in with Web3 ecosystem

**Cons**:
- As keys in JSON are often not ordered in implementations, not well suited for producing hashes for CIDs
- `0x` encoding doesn't fit in with multiformats
- Not size efficient

-----

- JSON based
- The `type` field contains the [EntityKind](./generated/rlay-ontology-entities) of the entity
- Values in non-CID bytes fields is not strictly defined but assumed to be CBOR encoded

Example:
```json
{
  "type": "Annotation",
  "property": "0x019780031b20b3179194677268c88cfd1644c6a1e100729465b42846a2bf7f0bddcd07e300a9",
  "value": "0x664b72616b656e"
}
```
