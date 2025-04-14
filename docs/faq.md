---
title: FAQs
---

# FAQs

#### **Can LumaLang target both Bitcoin and Stacks?**

Lumalanghooks can listen and act on events from the Bitcoin and Stacks network.

#### **Can I use lumaLang for cross-chain protocols?**

Yes, Lumalanghooks can be used for coordinating cross-chain actions. You can use lumaLang on Bitcoin, ordinals, and Stacks.

#### **Can I use lumaLang for chain-indexing?**

LumaLang can easily extract the information needed to build (or rebuild) databases for a front end.

#### **Can I use LumaLang with distributed nodes?**

The lumaLang event observer was designed as a library written in Rust, which makes it very portable. Bindings can easily be created from other languages (Node, Ruby, Python, etc.), making this tool a very convenient and performant library, usable by anyone.

#### **How can I connect lumaLang with Oracles?**

Oracles, in general, do the following:

 1. Capture relevant on-chain events
 2. Process the events via some off-chain, centralized logic
 3. Commit the resultant data on-chain

 LumaLang can be used to efficiently capture relevant on-chain events and forward them to off-chain services.

#### **How can I use LumaLang in my application?**

LumaLang can be used from the exposed RESTful API endpoints. A comprehensive OpenAPI specification explaining how to interact with the LumaLang REST API can be found [here](https://raw.githubusercontent.com/luminachain/lumaLang/develop/docs/lumaLang-openapi.json).

#### **Can I run lumaLang on mainnet?**

Yes, you can run lumaLang on both the testnet and mainnet.

#### **How can I optimize lumaLang scanning?**

Use adequate values for `start_block` and `end_block` in predicates by reducing the number of network hops between the lumaLang and the `bitcoind` process.
