/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use anyhow::ensure;
use anyhow::Context;
use anyhow::Result;
use commit_transformation::rewrite_commit;
use commit_transformation::RewriteOpts;
use context::CoreContext;
use itertools::Itertools;
use mononoke_types::BonsaiChangesetMut;
use mononoke_types::ChangesetId;
use movers::Movers;

use crate::commit_syncers_lib::mover_to_multi_mover;
use crate::commit_syncers_lib::CommitRewriteResult;
use crate::git_submodules::expand::expand_all_git_submodule_file_changes;
use crate::git_submodules::utils::get_submodule_expansions_affected;
use crate::git_submodules::validation::ValidSubmoduleExpansionBonsai;
use crate::reporting::set_scuba_logger_fields;
use crate::run_and_log_stats_to_scuba;
use crate::types::Repo;
use crate::SubmoduleExpansionData;

/// Sync a commit to/from a small repo with submodule expansion enabled.
pub async fn sync_commit_with_submodule_expansion<'a, R: Repo>(
    ctx: &'a CoreContext,
    bonsai: BonsaiChangesetMut,
    source_repo: &'a R,
    sm_exp_data: SubmoduleExpansionData<'a, R>,
    movers: Movers,
    // Parameters needed to generate a bonsai for the large repo using `rewrite_commit`
    remapped_parents: &'a HashMap<ChangesetId, ChangesetId>,
    rewrite_opts: RewriteOpts,
) -> Result<CommitRewriteResult> {
    let is_forward_sync =
        source_repo.repo_identity().id() != sm_exp_data.large_repo.repo_identity().id();

    if !is_forward_sync {
        let ctx = &set_scuba_logger_fields(
            ctx,
            [
                (
                    "source_repo",
                    sm_exp_data.large_repo.repo_identity().id().id(),
                ),
                ("target_repo", sm_exp_data.small_repo_id.id()),
            ],
        );
        return backsync_without_submodule_expansion_support(
            ctx,
            bonsai,
            sm_exp_data,
            source_repo,
            movers,
            remapped_parents,
            rewrite_opts,
        )
        .await;
    };

    let ctx = &set_scuba_logger_fields(
        ctx,
        [
            ("source_repo", source_repo.repo_identity().id().id()),
            (
                "target_repo",
                sm_exp_data.large_repo.repo_identity().id().id(),
            ),
        ],
    );

    let (new_bonsai, submodule_expansion_content_ids) = run_and_log_stats_to_scuba(
        ctx,
        "Expanding all git submodule file changes",
        None,
        expand_all_git_submodule_file_changes(ctx, bonsai, source_repo, sm_exp_data.clone()),
    )
    .await
    .context("Failed to expand submodule file changes from bonsai")?;

    let mb_rewritten_bonsai = run_and_log_stats_to_scuba(
        ctx,
        "Rewriting commit",
        None,
        rewrite_commit(
            ctx,
            new_bonsai,
            remapped_parents,
            mover_to_multi_mover(movers.mover.clone()),
            source_repo,
            None,
            rewrite_opts,
        ),
    )
    .await
    .context("Failed to rewrite commit")?;

    match mb_rewritten_bonsai {
        Some(rewritten_bonsai) => {
            let rewritten_bonsai = rewritten_bonsai.freeze()?;

            let validated_bonsai = run_and_log_stats_to_scuba(
                ctx,
                "Validating all submodule expansions",
                None,
                ValidSubmoduleExpansionBonsai::validate_all_submodule_expansions(
                    ctx,
                    sm_exp_data,
                    rewritten_bonsai,
                    movers.mover,
                ),
            )
            .await
            // TODO(gustavoavena): print some identifier of changeset that failed
            .context("Validation of submodule expansion failed")?;

            let rewritten = Some(validated_bonsai.into_inner().into_mut());

            Ok(CommitRewriteResult::new(
                rewritten,
                submodule_expansion_content_ids,
            ))
        }
        None => Ok(CommitRewriteResult::new(None, HashMap::new())),
    }
}

/// Sync a commit from large to small repo **only if it doesn't modify any
/// submodule expansion**.
async fn backsync_without_submodule_expansion_support<'a, R: Repo>(
    ctx: &'a CoreContext,
    bonsai_mut: BonsaiChangesetMut,
    sm_exp_data: SubmoduleExpansionData<'a, R>,
    source_repo: &'a R,
    movers: Movers,
    remapped_parents: &'a HashMap<ChangesetId, ChangesetId>,
    rewrite_opts: RewriteOpts,
) -> Result<CommitRewriteResult> {
    let submodules_affected =
        get_submodule_expansions_affected(&sm_exp_data, &bonsai_mut, movers.mover.clone())?;

    ensure!(
        submodules_affected.is_empty(),
        "Changeset can't be synced from large to small repo because it modifies the expansion of submodules: {0:#?}",
        submodules_affected
            .into_iter()
            .map(|p| p.to_string())
            .sorted()
            .collect::<Vec<_>>(),
    );

    let rewriten = rewrite_commit(
        ctx,
        bonsai_mut,
        remapped_parents,
        mover_to_multi_mover(movers.mover.clone()),
        source_repo,
        None,
        rewrite_opts,
    )
    .await
    .context("Failed to create small repo bonsai")?;

    Ok(CommitRewriteResult::new(rewriten, HashMap::new()))
}
