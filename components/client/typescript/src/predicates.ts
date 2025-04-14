import * as fs from 'fs/promises';
import * as path from 'path';
import { logger } from './util/logger';
import {
  Predicate,
  PredicateSchema,
  SerializedPredicateResponse,
  ThenThatHttpPost,
} from './schemas/predicate';
import { request } from 'undici';
import { TypeCompiler } from '@sinclair/typebox/compiler';
import { LumalanghookNodeOptions, EventObserverOptions, EventObserverPredicate } from '.';
import { randomUUID } from 'crypto';

/** Keeps the on-disk predicates in memory for faster access. */
const RegisteredPredicates = new Map<string, Predicate>();

const CompiledPredicateSchema = TypeCompiler.Compile(PredicateSchema);

// Async version of fs.existsSync
async function pathExists(path: string): Promise<boolean> {
  try {
    await fs.access(path);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return false;
    }
    throw error; // Re-throw other errors (e.g., permission issues)
  }
}

/**
 * Looks on disk and returns a map of registered Predicates, where the key is the predicate `name`
 * as defined by the user.
 */
export async function recallPersistedPredicatesFromDisk(
  basePath: string
): Promise<Map<string, Predicate>> {
  RegisteredPredicates.clear();
  try {
    if (!(await pathExists(basePath))) return RegisteredPredicates;
    for (const file of await fs.readdir(basePath)) {
      if (file.endsWith('.json')) {
        const text = await fs.readFile(path.join(basePath, file), 'utf-8');
        const predicate = JSON.parse(text) as JSON;
        if (CompiledPredicateSchema.Check(predicate)) {
          logger.info(
            `LumalanghookEventObserver recalled predicate '${predicate.name}' (${predicate.uuid}) from disk`
          );
          RegisteredPredicates.set(predicate.name, predicate);
        }
      }
    }
  } catch (error) {
    logger.error(error, `LumalanghookEventObserver unable to retrieve persisted predicates from disk`);
    RegisteredPredicates.clear();
  }
  return RegisteredPredicates;
}

export async function savePredicateToDisk(basePath: string, predicate: Predicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.name)}.json`;
  try {
    await fs.mkdir(basePath, { recursive: true });
    await fs.writeFile(predicatePath, JSON.stringify(predicate, null, 2));
    logger.info(
      `LumalanghookEventObserver persisted predicate '${predicate.name}' (${predicate.uuid}) to disk`
    );
  } catch (error) {
    logger.error(
      error,
      `LumalanghookEventObserver unable to persist predicate '${predicate.name}' (${predicate.uuid}) to disk`
    );
  }
}

async function deletePredicateFromDisk(basePath: string, predicate: Predicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.name)}.json`;
  try {
    await fs.rm(predicatePath);
    logger.info(
      `LumalanghookEventObserver deleted predicate '${predicate.name}' (${predicate.uuid}) from disk`
    );
  } catch (error: unknown) {
    // ignore if the file doesn't exist
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') {
      logger.error(error, `Failed to delete predicate`);
    }
  }
}

/** Checks the LumaLang node to see if a predicate is still valid and active */
async function isPredicateActive(
  predicate: Predicate,
  lumaLang: LumalanghookNodeOptions
): Promise<boolean | undefined> {
  try {
    const result = await request(`${lumaLang.base_url}/v1/lumalanghooks/${predicate.uuid}`, {
      method: 'GET',
      headers: { accept: 'application/json' },
      throwOnError: true,
    });
    const response = (await result.body.json()) as SerializedPredicateResponse;
    if (response.status == 404) return undefined;
    if (
      response.result.enabled == false ||
      response.result.status.type == 'interrupted' ||
      response.result.status.type == 'unconfirmed_expiration' ||
      response.result.status.type == 'confirmed_expiration'
    ) {
      return false;
    }
    return true;
  } catch (error) {
    logger.error(
      error,
      `LumalanghookEventObserver unable to check if predicate '${predicate.name}' (${predicate.uuid}) is active`
    );
    return false;
  }
}

/**
 * Registers a predicate in the LumaLang server. Automatically handles pre-existing predicates
 * found on disk.
 */
async function registerPredicate(
  pendingPredicate: EventObserverPredicate,
  diskPredicates: Map<string, Predicate>,
  observer: EventObserverOptions,
  lumaLang: LumalanghookNodeOptions
) {
  // First check if we've already registered this predicate in the past, and if so, make sure it's
  // still active on the LumaLang server.
  if (observer.node_type === 'lumaLang') {
    const diskPredicate = diskPredicates.get(pendingPredicate.name);
    if (diskPredicate) {
      switch (await isPredicateActive(diskPredicate, lumaLang)) {
        case true:
          logger.debug(
            `LumalanghookEventObserver predicate '${diskPredicate.name}' (${diskPredicate.uuid}) is active`
          );
          return;
        case undefined:
          logger.info(
            `LumalanghookEventObserver predicate '${diskPredicate.name}' (${diskPredicate.uuid}) found on disk but not on the LumaLang server`
          );
          break;
        case false:
          logger.info(
            `LumalanghookEventObserver predicate '${diskPredicate.name}' (${diskPredicate.uuid}) was being used but is now inactive, removing for re-regristration`
          );
          await removePredicate(diskPredicate, observer, lumaLang);
          break;
      }
    }
  }

  logger.info(`LumalanghookEventObserver registering predicate '${pendingPredicate.name}'`);
  try {
    // Add the `uuid` and `then_that` portions to the predicate.
    const thenThat: ThenThatHttpPost = {
      http_post: {
        url: `${observer.external_base_url}/payload`,
        authorization_header: `Bearer ${observer.auth_token}`,
      },
    };
    let newPredicate = pendingPredicate as Predicate;
    newPredicate.uuid = randomUUID();
    if (newPredicate.networks.mainnet) newPredicate.networks.mainnet.then_that = thenThat;
    if (newPredicate.networks.testnet) newPredicate.networks.testnet.then_that = thenThat;

    if (observer.predicate_re_register_callback) {
      newPredicate = await observer.predicate_re_register_callback(newPredicate);
    }

    const path = observer.node_type === 'lumaLang' ? `/v1/lumalanghooks` : `/v1/observers`;
    await request(`${lumaLang.base_url}${path}`, {
      method: 'POST',
      body: JSON.stringify(newPredicate),
      headers: { 'content-type': 'application/json' },
      throwOnError: true,
    });
    logger.info(
      `LumalanghookEventObserver registered '${newPredicate.name}' predicate (${newPredicate.uuid})`
    );
    await savePredicateToDisk(observer.predicate_disk_file_path, newPredicate);
    RegisteredPredicates.set(newPredicate.name, newPredicate);
  } catch (error) {
    logger.error(error, `LumalanghookEventObserver unable to register predicate`);
  }
}

/** Removes a predicate from the LumaLang server */
async function removePredicate(
  predicate: Predicate,
  observer: EventObserverOptions,
  lumaLang: LumalanghookNodeOptions
): Promise<void> {
  const nodeType = observer.node_type ?? 'lumaLang';
  const path =
    nodeType === 'lumaLang'
      ? `/v1/lumalanghooks/${predicate.chain}/${encodeURIComponent(predicate.uuid)}`
      : `/v1/observers/${encodeURIComponent(predicate.uuid)}`;
  try {
    await request(`${lumaLang.base_url}${path}`, {
      method: 'DELETE',
      headers: { 'content-type': 'application/json' },
      throwOnError: true,
    });
    logger.info(`LumalanghookEventObserver removed predicate '${predicate.name}' (${predicate.uuid})`);
    await deletePredicateFromDisk(observer.predicate_disk_file_path, predicate);
  } catch (error) {
    logger.error(error, `LumalanghookEventObserver unable to deregister predicate`);
  }
}

/** Registers predicates with the LumaLang server when our event observer is booting up */
export async function registerAllPredicatesOnObserverReady(
  predicates: EventObserverPredicate[],
  observer: EventObserverOptions,
  lumaLang: LumalanghookNodeOptions
) {
  logger.info(predicates, `LumalanghookEventObserver connected to ${lumaLang.base_url}`);
  if (predicates.length === 0) {
    logger.info(`LumalanghookEventObserver does not have predicates to register`);
    return;
  }
  const diskPredicates = await recallPersistedPredicatesFromDisk(observer.predicate_disk_file_path);
  for (const predicate of predicates)
    await registerPredicate(predicate, diskPredicates, observer, lumaLang);
}

/** Removes predicates from the LumaLang server when our event observer is being closed */
export async function removeAllPredicatesOnObserverClose(
  observer: EventObserverOptions,
  lumaLang: LumalanghookNodeOptions
) {
  const diskPredicates = await recallPersistedPredicatesFromDisk(observer.predicate_disk_file_path);
  if (diskPredicates.size === 0) {
    logger.info(`LumalanghookEventObserver does not have predicates to close`);
    return;
  }
  logger.info(`LumalanghookEventObserver closing predicates at ${lumaLang.base_url}`);
  const removals = [...RegisteredPredicates.values()].map(predicate =>
    removePredicate(predicate, observer, lumaLang)
  );
  await Promise.allSettled(removals);
  RegisteredPredicates.clear();
}

export async function predicateHealthCheck(
  observer: EventObserverOptions,
  lumaLang: LumalanghookNodeOptions
): Promise<void> {
  logger.debug(`LumalanghookEventObserver performing predicate health check`);
  for (const predicate of RegisteredPredicates.values()) {
    // This will be a no-op if the predicate is already active.
    await registerPredicate(predicate, RegisteredPredicates, observer, lumaLang);
  }
}
