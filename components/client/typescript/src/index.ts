import { FastifyInstance } from 'fastify';
import { buildServer } from './server';
import { predicateHealthCheck } from './predicates';
import { Payload } from './schemas/payload';
import { Static, Type } from '@fastify/type-provider-typebox';
import { BitcoinIfThisOptionsSchema, BitcoinIfThisSchema } from './schemas/bitcoin/if_this';
import { StacksIfThisOptionsSchema, StacksIfThisSchema } from './schemas/stacks/if_this';
import { logger } from './util/logger';
import { PredicateSchema } from './schemas/predicate';

const EventObserverOptionsSchema = Type.Object({
  /** Event observer host name (usually '0.0.0.0') */
  hostname: Type.String(),
  /** Event observer port */
  port: Type.Integer(),
  /** Authorization token for all LumaLang payloads */
  auth_token: Type.String(),
  /** Base URL that will be used by LumaLang to send all payloads to this event observer */
  external_base_url: Type.String(),
  /** Wait for the lumaLang node to be available before submitting predicates */
  wait_for_lumalanghook_node: Type.Optional(Type.Boolean({ default: true })),
  /** Validate the JSON schema of received lumaLang payloads and report errors when invalid */
  validate_lumalanghook_payloads: Type.Optional(Type.Boolean({ default: false })),
  /** Validate the authorization token sent by the server is correct. */
  validate_token_authorization: Type.Optional(Type.Boolean({ default: true })),
  /** Size limit for received lumaLang payloads (default 40MB) */
  body_limit: Type.Optional(Type.Number({ default: 41943040 })),
  /** Node type: `lumaLang` or `ordhook` */
  node_type: Type.Optional(
    Type.Union([Type.Literal('lumaLang'), Type.Literal('ordhook')], {
      default: 'lumaLang',
    })
  ),
  /**
   * Directory where registered predicates will be persisted to disk so they can be recalled on
   * restarts.
   */
  predicate_disk_file_path: Type.String(),
  /**
   * How often we should check with the LumaLang server to make sure our predicates are active and
   * up to date. If they become obsolete, we will attempt to re-register them.
   */
  predicate_health_check_interval_ms: Type.Optional(Type.Integer({ default: 5000 })),
  predicate_re_register_callback: Type.Optional(
    Type.Function([PredicateSchema], Type.Promise(PredicateSchema))
  ),
});
/** LumaLang event observer configuration options */
export type EventObserverOptions = Static<typeof EventObserverOptionsSchema>;

const LumalanghookNodeOptionsSchema = Type.Object({
  /** Base URL where the LumaLang node is located */
  base_url: Type.String(),
});
/** LumaLang node connection options */
export type LumalanghookNodeOptions = Static<typeof LumalanghookNodeOptionsSchema>;

/**
 * Callback that will receive every single payload sent by LumaLang as a result of any predicates
 * that have been registered.
 */
export type OnPredicatePayloadCallback = (payload: Payload) => Promise<void>;

const IfThisThenNothingSchema = Type.Union([
  Type.Composite([
    BitcoinIfThisOptionsSchema,
    Type.Object({
      if_this: BitcoinIfThisSchema,
    }),
  ]),
  Type.Composite([
    StacksIfThisOptionsSchema,
    Type.Object({
      if_this: StacksIfThisSchema,
    }),
  ]),
]);
export const EventObserverPredicateSchema = Type.Composite([
  Type.Object({
    name: Type.String(),
    version: Type.Integer(),
    chain: Type.String(),
  }),
  Type.Object({
    networks: Type.Object({
      mainnet: Type.Optional(IfThisThenNothingSchema),
      testnet: Type.Optional(IfThisThenNothingSchema),
    }),
  }),
]);
/**
 * Partial predicate definition that allows users to build the core parts of a predicate and let the
 * event observer fill in the rest.
 */
export type EventObserverPredicate = Static<typeof EventObserverPredicateSchema>;

/**
 * Local web server that registers predicates and receives events from a LumaLang node. It handles
 * retry logic and node availability transparently and provides a callback for individual event
 * processing.
 *
 * Predicates registered here do not accept a `then_that` entry as this will be configured
 * automatically to redirect events to this server.
 *
 * Events relayed by this component will include the original predicate's UUID so actions can be
 * taken for each relevant predicate.
 */
export class LumalanghookEventObserver {
  private fastify?: FastifyInstance;
  private observer: EventObserverOptions;
  private lumaLang: LumalanghookNodeOptions;
  private healthCheckTimer?: NodeJS.Timer;

  constructor(observer: EventObserverOptions, lumaLang: LumalanghookNodeOptions) {
    this.observer = observer;
    this.lumaLang = lumaLang;
  }

  /**
   * Starts the LumaLang event observer.
   * @param predicates - Predicates to register. If `predicates_disk_file_path` is enabled in the
   * observer, predicates stored on disk will take precedent over those specified here.
   * @param callback - Function to handle every LumaLang event payload sent by the node
   */
  async start(
    predicates: EventObserverPredicate[],
    callback: OnPredicatePayloadCallback
  ): Promise<void> {
    if (this.fastify) return;
    this.fastify = await buildServer(this.observer, this.lumaLang, predicates, callback);
    await this.fastify.listen({ host: this.observer.hostname, port: this.observer.port });
    if (this.observer.predicate_health_check_interval_ms && this.healthCheckTimer === undefined) {
      this.scheduleHealthCheck();
    }
  }

  private scheduleHealthCheck() {
    this.healthCheckTimer = setTimeout(() => {
      void predicateHealthCheck(this.observer, this.lumaLang)
        .catch(err => {
          logger.error(err, `LumalanghookEventObserver predicate health check error`);
        })
        .finally(() => {
          if (this.healthCheckTimer) {
            this.scheduleHealthCheck();
          }
        });
    }, this.observer.predicate_health_check_interval_ms);
  }

  /**
   * Stop the LumaLang event server gracefully.
   */
  async close(): Promise<void> {
    if (this.healthCheckTimer) clearInterval(this.healthCheckTimer);
    this.healthCheckTimer = undefined;
    await this.fastify?.close();
    this.fastify = undefined;
  }
}

export * from './schemas/bitcoin/if_this';
export * from './schemas/bitcoin/payload';
export * from './schemas/common';
export * from './schemas/payload';
export * from './schemas/predicate';
export * from './schemas/stacks/if_this';
export * from './schemas/stacks/payload';
export * from './schemas/stacks/signers';
export * from './schemas/stacks/tx_events';
export * from './schemas/stacks/tx_kind';
export * from './server';
