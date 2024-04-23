/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import path from 'node:path';

/**
 * Add a trailing path sep (/ or \) if one does not exist
 */
export function ensureTrailingPathSep(p: string): string {
  if (p.endsWith(path.sep)) {
    return p;
  }
  return p + path.sep;
}

/**
 * Add a trailing path sep (/ or \) if one does not exist
 */
export function removeLeadingPathSep(p: string): string {
  if (p.startsWith(path.sep)) {
    return p.slice(1);
  }
  return p;
}
