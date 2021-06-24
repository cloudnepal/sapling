/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::Context;
use futures::prelude::*;

use async_runtime::block_unless_interrupted;
use http_client::Stats;

use crate::errors::EdenApiError;

pub use edenapi_trait::Fetch;
pub use edenapi_trait::ResponseMeta;

/// Non-async version of `Fetch`.
pub struct BlockingFetch<T> {
    pub entries: Vec<T>,
    pub stats: Stats,
}

impl<T> BlockingFetch<T> {
    pub(crate) fn from_async<F>(fetch: F) -> Result<Self, EdenApiError>
    where
        F: Future<Output = Result<Fetch<T>, EdenApiError>>,
    {
        let Fetch { entries, stats } =
            block_unless_interrupted(fetch).context("transfer interrupted by user")??;
        let entries = block_unless_interrupted(entries.try_collect())
            .context("transfer interrupted by user")??;
        let stats = block_unless_interrupted(stats).context("transfer interrupted by user")??;
        Ok(Self { entries, stats })
    }
}
