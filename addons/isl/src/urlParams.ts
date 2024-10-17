/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {InitialParamKeys} from './platform';

import {logger} from './logger';

const INITIAL_PARAMS_LOCAL_STORAGE_KEY = 'ISLInitialParams';

declare global {
  interface Window {
    relativeDateNowOverride?: number;
  }
}

/**
 * Extract parameters from URL, then remove from URL to be cleaner (and hide sensitive tokens)
 */
export function computeInitialParams(isBrowserPlatform: boolean): Map<InitialParamKeys, string> {
  let initialParams: Map<InitialParamKeys, string> | undefined;
  if (typeof window === 'undefined') {
    return new Map();
  }
  if (window.location.search) {
    initialParams = new Map([...new URLSearchParams(window.location.search).entries()]);
    logger.log('Loaded initial params from URL: ', initialParams);
    if (isBrowserPlatform) {
      // Save params to local storage so reloading the page keeps the same URL parameters
      // Note: this assumes if search parameters are provided, ALL relevant search parameters are provided at the same time.
      // This way initial parameters stored in local storage is always consistent.
      try {
        localStorage.setItem(
          INITIAL_PARAMS_LOCAL_STORAGE_KEY,
          JSON.stringify([...initialParams.entries()].filter(([k]) => k !== 'sessionId')),
        );
      } catch (error) {
        logger.log('Failed to save initial params to local storage', error);
      }
      window.history.replaceState({}, document.title, window.location.pathname);
      logger.log('Saved initial params to local storage');
    }
  }
  // if parameters not passed in the URL, load previously seen values from localStorage.
  if (!initialParams) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      initialParams = new Map(JSON.parse(localStorage.getItem(INITIAL_PARAMS_LOCAL_STORAGE_KEY)!));
      logger.log('Loaded initial params from local storage: ', initialParams);
    } catch (error) {
      logger.log('Failed to load initial params from local storage', error);
    }
  }

  // relative date's "now" override is stored separate in window for easier access
  const nowOverride = initialParams?.get('now');
  if (nowOverride) {
    try {
      window.relativeDateNowOverride = parseInt(nowOverride);
    } catch (error) {
      logger.error('relative date "now" override in the wrong format', error);
    }
  }

  return initialParams ?? new Map();
}
