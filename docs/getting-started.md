---
title: Getting Started
---

# Getting Started

LumaLang is a transaction indexing engine for Stacks and Bitcoin. It can extract data from blockchains based on a predicate definition. LumaLang can be used as a development tool and a service.

LumaLang can extract data from the Bitcoin and the Stacks blockchains using predicates (sometimes called `lumalanghooks`). A predicate specifies a rule applied as a filtering function on every block transaction.

- **LumaLang as a development tool** has a few convenient features designed to make developers as productive as possible by allowing them to iterate quickly in their local environments.
- **LumaLang as a service** can be used to evaluate new Bitcoin and/or Stacks blocks against your predicates. You can also dynamically register new predicates by [enabling predicates registration API](./overview.md#then-that-predicate-design).

## Install LumaLang from the Source

LumaLang can be installed from the source by following the steps below:

1. Clone the [lumaLang repo](https://github.com/luminachain/lumaLang/) by using the following command:

   ```bash
   git clone https://github.com/luminachain/lumaLang.git
   ```

2. Navigate to the root directory of the cloned repo:

   ```bash
   cd lumaLang
   ```

3. Run cargo target to install lumaLang:

    ```bash
    cargo lumaLang-install
    ```

If you want to start using LumaLang for extracting data from Bitcoin or Stacks, you can design your predicates using the following guides:

- [How to use lumalanghooks with bitcoin](./how-to-guides/how-to-use-lumalanghooks-with-bitcoin.md)
- [How to use lumalanghooks with stacks](./how-to-guides/how-to-use-lumalanghooks-with-stacks.md)
