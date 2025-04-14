# LumaLang Client

A TypeScript client library that allows you to configure and listen for fully-typed LumaLang
predicate events.

You can either create predicates and send them to a LumaLang node manually or use the included web
server helpers to handle all node interactions transparently.

## Quick Start

1. Install npm package
    ```
    npm install @luminachain/lumaLang-client
    ```

1. Create a `Predicate` object for every LumaLang event you're interested in. See the LumaLang
   [README](https://github.com/luminachain/lumaLang#readme) for more examples.
    ```typescript
    import { ServerPredicate } from "@luminachain/lumaLang-client";
    import { randomUUID } from "crypto";

    const uuid = randomUUID();
    const predicate: ServerPredicate = {
        uuid,
        name: "test",
        version: 1,
        chain: "stacks",
        networks: {
            mainnet: {
                // `then_that` will be filled in automatically.
                if_this: {
                    scope: 'block_height',
                    higher_than: 100000,
                }
            }
        }
    };
    ```

1. Create configuration objects for the local server and the LumaLang node you'll be interacting
   with
    ```typescript
    import { ServerOptions, LumalanghookNodeOptions } from "@luminachain/lumaLang-client";

    // Local server options
    const opts: ServerOptions = {
        hostname: "0.0.0.0",
        port: 3000,
        auth_token: "<random_string>",
        // Configure this value to a hostname the LumaLang node can use to reach our local server.
        // e.g. http://local.server:3000
        external_base_url: "<external_base_url>"
    };

    // LumaLang node options
    const lumaLang: LumalanghookNodeOptions = {
        base_url: "<node_base_url>"
    };
    ```

1. Declare and start the event server
    ```typescript
    import { Payload } from "@luminachain/lumaLang-client";

    const server = new LumalanghookEventObserver(opts, lumaLang);
    server.start(
        [predicate],
        async (uuid: string, payload: Payload) => {
            // This handler will be called for every lumaLang event received by our server
            console.log(uuid);
            console.log(payload);
        }
    )
        .catch(e => console.error(e));
    const close = async () => {
        await server.close();
    }
    process.once('SIGINT', close);
    process.once('unhandledRejection', close);
    process.once('uncaughtException', close);
    process.once('beforeExit', close);
    ```
    Make sure you close the server gracefully in the event of an error so we can de-register our
    predicates correctly from the remote LumaLang node.

## Type Definitions

This library includes type definitions for all possible LumaLang predicate configurations and event
payloads in the form of [Typebox](https://github.com/sinclairzx81/typebox) schemas. If you need to
compile or validate any of the supplied types, generate JSON schemas, etc., you can use all of
Typebox's tools to do so.
