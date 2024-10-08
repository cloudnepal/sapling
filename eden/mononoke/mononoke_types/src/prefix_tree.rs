/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::BTreeMap;

use maplit::btreemap;
use smallvec::SmallVec;
use smallvec::ToSmallVec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefixTree<V> {
    pub prefix: SmallVec<[u8; 24]>,
    pub value: Option<Box<V>>,
    pub edges: BTreeMap<u8, Self>,
}

impl<V> Default for PrefixTree<V> {
    fn default() -> Self {
        Self {
            prefix: Default::default(),
            value: Default::default(),
            edges: Default::default(),
        }
    }
}

/// Returns longest common prefix of a and b.
fn common_prefix<'a>(a: &'a [u8], b: &'a [u8]) -> &'a [u8] {
    let lcp = a.iter().zip(b.iter()).take_while(|(a, b)| a == b).count();
    // Panic safety: lcp is at most a.len()
    &a[..lcp]
}

impl<V> PrefixTree<V> {
    /// Returns the value associated with the given key, if any.
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<&V> {
        let mut node = self;
        let mut key = key.as_ref();
        loop {
            key = key.strip_prefix(node.prefix.as_ref())?;

            let (next_byte, rest) = match key.split_first() {
                Some((next_byte, rest)) => (next_byte, rest),
                None => return node.value.as_ref().map(|value| value.as_ref()),
            };

            node = node.edges.get(next_byte)?;
            key = rest;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_none() && self.edges.is_empty()
    }

    #[cfg(test)]
    pub fn into_vec(self) -> Vec<(String, V)> {
        self.into_iter()
            .map(|(key, value)| (String::from_utf8(key.to_vec()).unwrap(), value))
            .collect()
    }
}

impl<V: Default> PrefixTree<V> {
    /// Inserts a key-value pair into the prefix tree, replacing
    /// the value if the key already exists.
    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K, value: V) {
        *self.get_or_insert_default(key) = value;
    }

    /// Returns a mutable reference to the value associated with the given key,
    /// or inserts the default value if the key does not exist.
    pub fn get_or_insert_default<K: AsRef<[u8]>>(&mut self, key: K) -> &mut V {
        if self.is_empty() {
            self.prefix = key.as_ref().to_smallvec();
            return self.value.get_or_insert(Default::default());
        }

        let lcp: SmallVec<[u8; 24]> = common_prefix(&self.prefix, key.as_ref()).into();

        if lcp.len() < self.prefix.len() {
            // The new key is a prefix of the current node's prefix.
            // We need to split the current node's prefix by creating
            // a new node.
            if lcp.len() == key.as_ref().len() {
                self.edges = btreemap! {
                    self.prefix[lcp.len()] => Self {
                        prefix: self.prefix[lcp.len() + 1..].to_smallvec(),
                        value: self.value.take(),
                        edges: std::mem::take(&mut self.edges),
                    }
                };
                self.prefix = self.prefix[..lcp.len()].to_smallvec();
                return self.value.get_or_insert(Default::default());
            // The new key splits off from the prefix of the current node.
            // We need to split the prefix by creating a new node and create
            // a new child for the rest of the new key.
            } else {
                self.edges = btreemap! {
                    self.prefix[lcp.len()] => Self {
                        prefix: self.prefix[lcp.len() + 1..].to_smallvec(),
                        value: self.value.take(),
                        edges: std::mem::take(&mut self.edges),
                    },
                    key.as_ref()[lcp.len()] => Self {
                        prefix: key.as_ref()[lcp.len() + 1..].to_smallvec(),
                        value: None,
                        edges: Default::default(),
                    }
                };
                self.prefix = self.prefix[..lcp.len()].to_smallvec();
                return self
                    .edges
                    .get_mut(&key.as_ref()[lcp.len()])
                    .unwrap()
                    .value
                    .get_or_insert(Default::default());
            }
        } else {
            // The new key matches the current node's prefix.
            // Replace the current node's value with the new value.
            if lcp.len() == key.as_ref().len() {
                return self.value.get_or_insert(Default::default());
            } else {
                // The new key extends past the current node's prefix.
                // Insert the new key into the child prefix tree.
                return self
                    .edges
                    .entry(key.as_ref()[lcp.len()])
                    .or_default()
                    .get_or_insert_default(&key.as_ref()[lcp.len() + 1..]);
            }
        }
    }
}

/// A consuming ordered iterator over all key-value pairs of a PrefixTree.
// The iterator works by keeping the state of a depth first search performed
// on the prefix tree:
// - `prefixes` is a list of the prefix tree nodes' prefixes and edges from
// the root to the current node in the search.
// - `value` is the value of the current node in the search, if any.
// - `stack` contains an iterator for each level descended in the depth first
// search, to allow continuing the search after backtracking from a level.
pub struct PrefixTreeIntoIter<V> {
    prefixes: Vec<SmallVec<[u8; 24]>>,
    value: Option<Box<V>>,
    stack: Vec<std::collections::btree_map::IntoIter<u8, PrefixTree<V>>>,
}

impl<V> IntoIterator for PrefixTree<V> {
    type Item = (SmallVec<[u8; 24]>, V);
    type IntoIter = PrefixTreeIntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        PrefixTreeIntoIter {
            prefixes: vec![self.prefix],
            value: self.value,
            stack: vec![self.edges.into_iter()],
        }
    }
}

impl<V> Iterator for PrefixTreeIntoIter<V> {
    type Item = (SmallVec<[u8; 24]>, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(value) = self.value.take() {
                return Some((
                    self.prefixes
                        .iter()
                        .fold(SmallVec::new(), |mut accum, prefix| {
                            accum.extend_from_slice(prefix);
                            accum
                        }),
                    *value,
                ));
            }

            match self.stack.last_mut() {
                None => return None,
                Some(iter) => match iter.next() {
                    None => {
                        self.prefixes.pop();
                        self.stack.pop();
                    }
                    Some((next_byte, mut child)) => {
                        child.prefix.insert(0, next_byte);
                        self.prefixes.push(child.prefix);
                        self.value = child.value;
                        self.stack.push(child.edges.into_iter());
                    }
                },
            };
        }
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use mononoke_macros::mononoke;
    use quickcheck::quickcheck;

    use super::*;

    #[mononoke::test]
    fn test_prefix_tree() -> Result<()> {
        let mut prefix_tree: PrefixTree<i32> = Default::default();
        assert_eq!(prefix_tree.clone().into_vec(), vec![]);

        prefix_tree.insert("abcde", 1);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![("abcde".to_string(), 1)]
        );

        assert_eq!(prefix_tree.get("abcde"), Some(&1));
        assert_eq!(prefix_tree.get("abc"), None);

        prefix_tree.insert("abcdf", 2);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![("abcde".to_string(), 1), ("abcdf".to_string(), 2)]
        );

        prefix_tree.insert("bcdf", 3);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("abcde".to_string(), 1),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
            ]
        );

        assert_eq!(prefix_tree.get(""), None);
        assert_eq!(prefix_tree.get("bcdf"), Some(&3));
        assert_eq!(prefix_tree.get("zzzz"), None);

        prefix_tree.insert("abcde", 4);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
            ]
        );

        prefix_tree.insert("zzzz", 10);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        prefix_tree.insert("", 5);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        assert_eq!(prefix_tree.get(""), Some(&5));
        assert_eq!(prefix_tree.get("bcdf"), Some(&3));
        assert_eq!(prefix_tree.get("zzzz"), Some(&10));
        assert_eq!(prefix_tree.get("zzzx"), None);

        prefix_tree.insert("abc", 3);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abc".to_string(), 3),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        prefix_tree.insert("abbbbbb", 2);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abbbbbb".to_string(), 2),
                ("abc".to_string(), 3),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        prefix_tree.insert("abbbbbb", 2);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abbbbbb".to_string(), 2),
                ("abc".to_string(), 3),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        prefix_tree.insert("ac", 0);
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abbbbbb".to_string(), 2),
                ("abc".to_string(), 3),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("ac".to_string(), 0),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        *prefix_tree.get_or_insert_default("ac") = 60;
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abbbbbb".to_string(), 2),
                ("abc".to_string(), 3),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("ac".to_string(), 60),
                ("bcdf".to_string(), 3),
                ("zzzz".to_string(), 10),
            ]
        );

        prefix_tree.get_or_insert_default("test");
        assert_eq!(
            prefix_tree.clone().into_vec(),
            vec![
                ("".to_string(), 5),
                ("abbbbbb".to_string(), 2),
                ("abc".to_string(), 3),
                ("abcde".to_string(), 4),
                ("abcdf".to_string(), 2),
                ("ac".to_string(), 60),
                ("bcdf".to_string(), 3),
                ("test".to_string(), 0),
                ("zzzz".to_string(), 10),
            ]
        );

        Ok(())
    }

    quickcheck! {
        fn quickcheck_prefix_tree(entries: BTreeMap<Vec<u8>, i32>) -> bool {
            let mut prefix_tree = PrefixTree::default();
            for (k, v) in entries.clone() {
                prefix_tree.insert(k, v);
            }

            let prefix_tree_entries = prefix_tree.into_iter().map(|(k, v)| (k.into_vec(), v)).collect::<Vec<_>>();
            let entries = entries.into_iter().collect::<Vec<_>>();

            prefix_tree_entries == entries
        }
    }
}
