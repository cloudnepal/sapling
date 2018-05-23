// Copyright Facebook, Inc. 2017

use errors::Result;
use filestate::FileStateV2;
use filestore::FileStore;
use serialization::Serializable;
use std::io::Cursor;
use std::ops::Deref;
use std::path::Path;
use store::{BlockId, Store, StoreView};
use tree::Tree;

/// `TreeState` uses a single tree to track an extended state of `TreeDirstate`.
/// See the comment about `FileStateV2` for the difference.
/// In short, `TreeState` combines dirstate and fsmonitor state.
pub struct TreeState {
    store: FileStore,
    tree: Tree<FileStateV2>,
    root: TreeStateRoot,
}

/// `TreeStateRoot` contains block id to the root `Tree`, and other metadata.
#[derive(Default)]
pub(crate) struct TreeStateRoot {
    pub version: u32,
    pub file_count: u32,
    pub tree_block_id: BlockId,
    pub watchman_clock: Box<[u8]>,
}

impl TreeState {
    /// Read `TreeState` from a file, or create an empty new `TreeState` if `root_id` is None.
    pub fn open<P: AsRef<Path>>(path: P, root_id: Option<BlockId>) -> Result<Self> {
        match root_id {
            Some(root_id) => {
                let store = FileStore::open(path)?;
                let root = {
                    let mut root_buf = Cursor::new(store.read(root_id)?);
                    TreeStateRoot::deserialize(&mut root_buf)?
                };
                let tree = Tree::open(root.tree_block_id, root.file_count);
                Ok(TreeState { store, tree, root })
            }
            None => {
                let store = FileStore::create(path)?;
                let root = TreeStateRoot::default();
                let tree = Tree::new();
                Ok(TreeState { store, tree, root })
            }
        }
    }

    /// Flush dirty entries. Return new `root_id` that can be passed to `open`.
    pub fn flush(&mut self) -> Result<BlockId> {
        let tree_block_id = { self.tree.write_delta(&mut self.store)? };
        self.write_root(tree_block_id)
    }

    /// Save as a new file.
    pub fn write_as<P: AsRef<Path>>(&mut self, path: P) -> Result<BlockId> {
        let mut new_store = FileStore::create(path)?;
        let tree_block_id = self.tree.write_full(&mut new_store, &self.store)?;
        self.store = new_store;
        let root_id = self.write_root(tree_block_id)?;
        Ok(root_id)
    }

    fn write_root(&mut self, tree_block_id: BlockId) -> Result<BlockId> {
        self.root.tree_block_id = tree_block_id;
        self.root.file_count = self.len() as u32;

        let mut root_buf = Vec::new();
        self.root.serialize(&mut root_buf)?;
        let result = self.store.append(&root_buf)?;
        self.store.flush()?;
        Ok(result)
    }

    /// Create or replace the existing entry.
    pub fn insert<K: AsRef<[u8]>>(&mut self, path: K, state: &FileStateV2) -> Result<()> {
        self.tree.add(&self.store, path.as_ref(), state)
    }

    pub fn remove<K: AsRef<[u8]>>(&mut self, path: K) -> Result<bool> {
        self.tree.remove(&self.store, path.as_ref())
    }

    pub fn get<K: AsRef<[u8]>>(&mut self, path: K) -> Result<Option<&FileStateV2>> {
        self.tree.get(&self.store, path.as_ref())
    }

    pub fn get_mut<K: AsRef<[u8]>>(&mut self, path: K) -> Result<Option<&mut FileStateV2>> {
        self.tree.get_mut(&self.store, path.as_ref())
    }

    pub fn len(&self) -> usize {
        self.tree.file_count() as usize
    }

    pub fn set_watchman_clock<T: AsRef<[u8]>>(&mut self, clock: T) {
        self.root.watchman_clock = Vec::from(clock.as_ref()).into_boxed_slice();
    }

    pub fn get_watchman_clock(&self) -> &[u8] {
        self.root.watchman_clock.deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{ChaChaRng, Rng};
    use tempdir::TempDir;

    #[test]
    fn test_new() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let state = TreeState::open(dir.path().join("empty"), None).expect("open");
        assert!(state.get_watchman_clock().is_empty());
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_empty_flush() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = TreeState::open(dir.path().join("empty"), None).expect("open");
        let block_id = state.flush().expect("flush");
        let state = TreeState::open(dir.path().join("empty"), block_id.into()).expect("open");
        assert!(state.get_watchman_clock().is_empty());
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_empty_write_as() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = TreeState::open(dir.path().join("empty"), None).expect("open");
        let block_id = state.write_as(dir.path().join("as")).expect("write_as");
        let state = TreeState::open(dir.path().join("as"), block_id.into()).expect("open");
        assert!(state.get_watchman_clock().is_empty());
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_set_watchman_clock() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = TreeState::open(dir.path().join("1"), None).expect("open");
        state.set_watchman_clock(b"foobar");
        let block_id1 = state.flush().expect("flush");
        let block_id2 = state.write_as(dir.path().join("2")).expect("write_as");
        let state = TreeState::open(dir.path().join("1"), block_id1.into()).expect("open");
        assert_eq!(state.get_watchman_clock()[..], b"foobar"[..]);
        let state = TreeState::open(dir.path().join("2"), block_id2.into()).expect("open");
        assert_eq!(state.get_watchman_clock()[..], b"foobar"[..]);
    }

    // Some random paths extracted from fb-hgext, plus some manually added entries, shuffled.
    const SAMPLE_PATHS: [&[u8]; 22] = [
        b".fbarcanist",
        b"build/.",
        b"phabricator/phabricator_graphql_client_urllib.pyc",
        b"hgext3rd/__init__.py",
        b"hgext3rd/.git/objects/14/8f179e7e702ddedb54c53f2726e7f81b14a33f",
        b"rust/radixbuf/.git/objects/pack/pack-c0bc37a255e59f5563de9a76013303d8df46a659.idx",
        b".hg/shelved/default-106.patch",
        b"rust/radixbuf/.git/objects/20/94e0274ba1ef2ec30de884e3ca4d7093838064",
        b"rust/radixbuf/.git/hooks/prepare-commit-msg.sample",
        b"rust/radixbuf/.git/objects/b3/9acb828290b77704cc44e748d6e7d4a528d6ae",
        b"scripts/lint.py",
        b".fbarcanist/unit/MercurialTestEngine.php",
        b".hg/shelved/default-37.patch",
        b"rust/radixbuf/.git/objects/01/d8e75b3bae0819c4095ae96ebdc889e9e5d806",
        b"hgext3rd/fastannotate/error.py",
        b"rust/radixbuf/.git/objects/pack/pack-c0bc37a255e59f5563de9a76013303d8df46a659.pack",
        b"distutils_rust/__init__.py",
        b".editorconfig",
        b"rust/radixbuf/.git/objects/01/89a583d7e9aff802cdfed3ff3cc3a473253281",
        b"hgext3rd/fastannotate/commands.py",
        b"distutils_rust/__init__.pyc",
        b"rust/radixbuf/.git/objects/b3/9b2824f47b66462e92ffa4f978bc95f5fdad2e",
    ];

    fn new_treestate<P: AsRef<Path>>(path: P) -> TreeState {
        let mut state = TreeState::open(path, None).expect("open");
        let mut rng = ChaChaRng::new_unseeded();
        for path in &SAMPLE_PATHS {
            let file = rng.gen();
            state.insert(path, &file).expect("insert");
        }
        state
    }

    #[test]
    fn test_insert() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = new_treestate(dir.path().join("1"));
        let mut rng = ChaChaRng::new_unseeded();
        for path in &SAMPLE_PATHS {
            let file: FileStateV2 = rng.gen();
            assert_eq!(state.get(path).unwrap().unwrap(), &file);
        }
        assert_eq!(state.len(), SAMPLE_PATHS.len());
    }

    #[test]
    fn test_remove() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = new_treestate(dir.path().join("1"));
        for path in &SAMPLE_PATHS {
            assert!(state.remove(path).unwrap())
        }
        for path in &SAMPLE_PATHS {
            assert!(!state.remove(path).unwrap())
        }
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_get_mut() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = new_treestate(dir.path().join("1"));
        for path in &SAMPLE_PATHS {
            let file = state.get_mut(path).unwrap().unwrap();
            file.mode ^= 3;
        }
        let mut rng = ChaChaRng::new_unseeded();
        for path in &SAMPLE_PATHS {
            let mut file: FileStateV2 = rng.gen();
            file.mode ^= 3;
            assert_eq!(state.get(path).unwrap().unwrap(), &file);
        }
    }

    #[test]
    fn test_non_empty_flush() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = new_treestate(dir.path().join("1"));
        let block_id = state.flush().expect("flush");
        let mut state = TreeState::open(dir.path().join("1"), block_id.into()).expect("open");
        let mut rng = ChaChaRng::new_unseeded();
        for path in &SAMPLE_PATHS {
            let file: FileStateV2 = rng.gen();
            assert_eq!(state.get(path).unwrap().unwrap(), &file);
        }
        assert_eq!(state.len(), SAMPLE_PATHS.len());
    }

    #[test]
    fn test_non_empty_write_as() {
        let dir = TempDir::new("treestate").expect("tempdir");
        let mut state = new_treestate(dir.path().join("1"));
        let block_id = state.write_as(dir.path().join("as")).expect("write_as");
        let mut state = TreeState::open(dir.path().join("as"), block_id.into()).expect("open");
        let mut rng = ChaChaRng::new_unseeded();
        for path in &SAMPLE_PATHS {
            let file: FileStateV2 = rng.gen();
            assert_eq!(state.get(path).unwrap().unwrap(), &file);
        }
        assert_eq!(state.len(), SAMPLE_PATHS.len());
    }
}
