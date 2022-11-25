use ruc::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env::temp_dir;
use std::ops::Bound::{Excluded, Included};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use storage::db::{DbIter, IterOrder, KVBatch, KValue, MerkleDB};

/// Wraps a Findora db instance and deletes it from disk it once it goes out of scope.
#[derive(Serialize, Deserialize)]
pub struct MemoryDB {
    temp: PathBuf,
    cache: BTreeMap<Box<[u8]>, Option<Box<[u8]>>>,
    inner: BTreeMap<Box<[u8]>, Option<Box<[u8]>>>,
    aux: BTreeMap<Box<[u8]>, Option<Box<[u8]>>>,
}

impl MemoryDB {
    pub fn new() -> MemoryDB {
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = temp_dir();
        path.push(format!("temp-memorydb–{}", time));
        MemoryDB {
            temp: path,
            cache: BTreeMap::new(),
            inner: BTreeMap::new(),
            aux: BTreeMap::new(),
        }
    }

    /// Opens a `MemoryDB` at an autogenerated, temporary file path.
    pub fn open(path: PathBuf) -> Result<MemoryDB> {
        if path.exists() {
            let bytes = std::fs::read(path).map_err(|_e| eg!("file missing"))?;
            bincode::deserialize(&bytes).map_err(|_e| eg!("deserialize failure"))
        } else {
            Ok(MemoryDB {
                temp: path,
                cache: BTreeMap::new(),
                inner: BTreeMap::new(),
                aux: BTreeMap::new(),
            })
        }
    }

    /// Closes db and deletes all data from disk.
    pub fn destroy(&mut self) {
        let _ = std::fs::remove_file(&self.temp);
        self.cache.clear();
        self.inner.clear();
    }
}

impl Default for MemoryDB {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleDB for MemoryDB {
    fn root_hash(&self) -> Vec<u8> {
        vec![]
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let k = key.to_vec().into_boxed_slice();
        Ok(self.inner.get(&k).cloned().flatten().map(|v| v.to_vec()))
    }

    fn get_aux(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let k = key.to_vec().into_boxed_slice();
        Ok(self.aux.get(&k).cloned().flatten().map(|v| v.to_vec()))
    }

    fn put_batch(&mut self, kvs: KVBatch) -> Result<()> {
        for (k, v) in kvs {
            self.inner
                .insert(k.into_boxed_slice(), v.map(|v| v.into_boxed_slice()));
        }
        Ok(())
    }

    fn db_all_iterator(&self, order: IterOrder) -> DbIter<'_>
    {
        let lower_key: &[u8] = b"0";

        let lower = lower_key.to_vec().into_boxed_slice();
        let upper =  lower_key.to_vec().into_boxed_slice();

        match order {
            IterOrder::Asc => Box::new(
                self.inner
                    .range::<Box<[u8]>, _>((Included(&lower), Excluded(&upper)))
                    .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone()))),
            ),
            IterOrder::Desc => Box::new(
                self.inner
                    .range::<Box<[u8]>, _>((Included(&lower), Excluded(&upper)))
                    .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
                    .rev(),
            ),
        }
    }

    fn iter(&self, lower: &[u8], upper: &[u8], order: IterOrder) -> DbIter<'_> {
        let lower = lower.to_vec().into_boxed_slice();
        let upper = upper.to_vec().into_boxed_slice();

        match order {
            IterOrder::Asc => Box::new(
                self.inner
                    .range::<Box<[u8]>, _>((Included(&lower), Excluded(&upper)))
                    .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone()))),
            ),
            IterOrder::Desc => Box::new(
                self.inner
                    .range::<Box<[u8]>, _>((Included(&lower), Excluded(&upper)))
                    .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
                    .rev(),
            ),
        }
    }

    fn iter_aux(&self, lower: &[u8], upper: &[u8], order: IterOrder) -> DbIter<'_> {
        let lower = lower.to_vec().into_boxed_slice();
        let upper = upper.to_vec().into_boxed_slice();

        match order {
            IterOrder::Asc => Box::new(
                self.aux
                    .range::<Box<[u8]>, _>((Included(&lower), Excluded(&upper)))
                    .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone()))),
            ),
            IterOrder::Desc => Box::new(
                self.aux
                    .range::<Box<[u8]>, _>((Included(&lower), Excluded(&upper)))
                    .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
                    .rev(),
            ),
        }
    }

    fn commit(&mut self, aux: KVBatch, flush: bool) -> Result<()> {
        for (k, v) in aux {
            self.aux
                .insert(k.into_boxed_slice(), v.map(|v| v.into_boxed_slice()));
        }
        if flush {
            let bytes = bincode::serialize(self).map_err(|_e| eg!("serialize failure"))?;
            std::fs::write(&self.temp, bytes).map_err(|_e| eg!("write file failure"))?;
        }
        Ok(())
    }

    fn snapshot<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let bytes = bincode::serialize(self).map_err(|_e| eg!("serialize failure"))?;
        std::fs::write(path, bytes).map_err(|_e| eg!("write file failure"))
    }

    fn decode_kv(&self, kv_pair: (Box<[u8]>, Box<[u8]>)) -> KValue {
        (kv_pair.0.to_vec(), kv_pair.1.to_vec())
    }

    fn clean_aux(&mut self) -> Result<()> {
        self.aux.clear();
        Ok(())
    }
}

impl Drop for MemoryDB {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryDB;
    use std::env::temp_dir;
    use std::time::SystemTime;
    use storage::db::{IterOrder, MerkleDB};

    #[test]
    fn db_put_n_get() {
        let mut fdb = MemoryDB::new();

        // put data
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v10".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
        ])
        .unwrap();
        // commit data with aux
        fdb.commit(vec![(b"height".to_vec(), Some(b"100".to_vec()))], false)
            .unwrap();

        // get and compare
        assert_eq!(fdb.get(b"k10").unwrap().unwrap(), b"v10".to_vec());
        assert_eq!(fdb.get(b"k20").unwrap().unwrap(), b"v20".to_vec());
        assert_eq!(fdb.get_aux(b"height").unwrap().unwrap(), b"100".to_vec());
    }

    #[test]
    fn db_del_n_get() {
        let mut fdb = MemoryDB::new();

        // put data
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v10".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
        ])
        .unwrap();
        // commit data with aux
        fdb.commit(vec![(b"height".to_vec(), Some(b"100".to_vec()))], false)
            .unwrap();

        // del data at height 101
        fdb.put_batch(vec![(b"k10".to_vec(), None), (b"k20".to_vec(), None)])
            .unwrap();
        // commit data with aux
        fdb.commit(vec![(b"height".to_vec(), Some(b"101".to_vec()))], false)
            .unwrap();

        // get and compare
        assert_eq!(fdb.get(b"k10").unwrap(), None);
        assert_eq!(fdb.get(b"k20").unwrap(), None);
        assert_eq!(fdb.get_aux(b"height").unwrap().unwrap(), b"101".to_vec());
    }

    #[test]
    fn db_put_n_update() {
        let mut fdb = MemoryDB::new();

        // put data
        fdb.put_batch(vec![(b"k10".to_vec(), Some(b"v10".to_vec()))])
            .unwrap();
        // commit data with aux
        fdb.commit(vec![(b"height".to_vec(), Some(b"100".to_vec()))], false)
            .unwrap();

        // update data at height
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v12".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
        ])
        .unwrap();
        // commit data with aux
        fdb.commit(vec![(b"height".to_vec(), Some(b"101".to_vec()))], false)
            .unwrap();

        // get and compare
        assert_eq!(fdb.get(b"k10").unwrap(), Some(b"v12".to_vec()));
        assert_eq!(fdb.get(b"k20").unwrap(), Some(b"v20".to_vec()));
        assert_eq!(fdb.get_aux(b"height").unwrap().unwrap(), b"101".to_vec());
    }

    #[test]
    fn del_n_iter_range() {
        let mut fdb = MemoryDB::new();

        // put data and commit
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v10".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
            (b"k30".to_vec(), Some(b"v30".to_vec())),
            (b"k40".to_vec(), Some(b"v40".to_vec())),
            (b"k50".to_vec(), Some(b"v50".to_vec())),
        ])
        .unwrap();
        fdb.commit(vec![(b"height".to_vec(), Some(b"100".to_vec()))], false)
            .unwrap();

        // del data at height 101
        fdb.put_batch(vec![(b"k20".to_vec(), None), (b"k40".to_vec(), None)])
            .unwrap();
        // commit data with aux
        fdb.commit(vec![(b"height".to_vec(), Some(b"101".to_vec()))], false)
            .unwrap();

        // iterate data on range ["k10", "k50")
        let iter = fdb.iter(b"k10", b"k50", IterOrder::Asc);
        let expected = vec![
            (b"k10".to_vec(), b"v10".to_vec()),
            (b"k30".to_vec(), b"v30".to_vec()),
        ];
        let actual = iter
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected, actual);
        assert_eq!(fdb.get_aux(b"height").unwrap().unwrap(), b"101".to_vec());
    }

    #[test]
    fn iter_range_inc() {
        let mut fdb = MemoryDB::new();

        // put data
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v10".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
            (b"k30".to_vec(), Some(b"v30".to_vec())),
            (b"k40".to_vec(), Some(b"v40".to_vec())),
            (b"k50".to_vec(), Some(b"v50".to_vec())),
        ])
        .unwrap();
        // commit data with aux
        fdb.commit(
            vec![
                (b"k11".to_vec(), Some(b"v11".to_vec())),
                (b"k21".to_vec(), Some(b"v21".to_vec())),
                (b"k31".to_vec(), Some(b"v31".to_vec())),
                (b"k41".to_vec(), Some(b"v41".to_vec())),
                (b"k51".to_vec(), Some(b"v51".to_vec())),
            ],
            true,
        )
        .unwrap();

        // iterate data on range ["k20", "k50")
        let iter = fdb.iter(b"k20", b"k50", IterOrder::Asc);
        let expected = vec![
            (b"k20".to_vec(), b"v20".to_vec()),
            (b"k30".to_vec(), b"v30".to_vec()),
            (b"k40".to_vec(), b"v40".to_vec()),
        ];
        let actual = iter
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected, actual);

        // iterate aux on range ["k21", "k51")
        let iter_aux = fdb.iter_aux(b"k21", b"k51", IterOrder::Asc);
        let expected_aux = vec![
            (b"k21".to_vec(), b"v21".to_vec()),
            (b"k31".to_vec(), b"v31".to_vec()),
            (b"k41".to_vec(), b"v41".to_vec()),
        ];
        let actual_aux = iter_aux
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected_aux, actual_aux);
    }

    #[test]
    fn iter_range_desc() {
        let mut fdb = MemoryDB::new();

        // put data and commit
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v10".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
            (b"k30".to_vec(), Some(b"v30".to_vec())),
            (b"k40".to_vec(), Some(b"v40".to_vec())),
            (b"k50".to_vec(), Some(b"v50".to_vec())),
        ])
        .unwrap();
        fdb.commit(vec![], false).unwrap();

        // iterate data on range ["k20", "k50")
        let iter = fdb.iter(b"k20", b"k50", IterOrder::Desc);
        let expected = vec![
            (b"k40".to_vec(), b"v40".to_vec()),
            (b"k30".to_vec(), b"v30".to_vec()),
            (b"k20".to_vec(), b"v20".to_vec()),
        ];
        let actual = iter
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected, actual);

        // commit aux
        fdb.commit(
            vec![
                (b"k11".to_vec(), Some(b"v11".to_vec())),
                (b"k21".to_vec(), Some(b"v21".to_vec())),
                (b"k31".to_vec(), Some(b"v31".to_vec())),
                (b"k41".to_vec(), Some(b"v41".to_vec())),
                (b"k51".to_vec(), Some(b"v51".to_vec())),
            ],
            false,
        )
        .unwrap();

        // iterate aux on range ["k21", "k51")
        let iter_aux = fdb.iter_aux(b"k21", b"k51", IterOrder::Desc);
        let expected_aux = vec![
            (b"k41".to_vec(), b"v41".to_vec()),
            (b"k31".to_vec(), b"v31".to_vec()),
            (b"k21".to_vec(), b"v21".to_vec()),
        ];
        let actual_aux = iter_aux
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected_aux, actual_aux);
    }

    #[test]
    fn db_snapshot() {
        let mut fdb = MemoryDB::new();

        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = temp_dir();
        path.push(format!("temp-memorydb–{}", time));

        // put data
        fdb.put_batch(vec![
            (b"k10".to_vec(), Some(b"v10".to_vec())),
            (b"k20".to_vec(), Some(b"v20".to_vec())),
            (b"k30".to_vec(), Some(b"v30".to_vec())),
            (b"k40".to_vec(), Some(b"v40".to_vec())),
        ])
        .unwrap();

        // commit with some aux
        fdb.commit(
            vec![
                (b"k11".to_vec(), Some(b"v11".to_vec())),
                (b"k21".to_vec(), Some(b"v21".to_vec())),
                (b"k31".to_vec(), Some(b"v31".to_vec())),
            ],
            false,
        )
        .unwrap();

        // take snapshot
        fdb.snapshot(&path).unwrap();

        // verify data
        let fdb_cp = MemoryDB::open(path).unwrap();

        assert_eq!(fdb_cp.get(b"k10").unwrap().unwrap(), b"v10".to_vec());
        assert_eq!(fdb_cp.get(b"k20").unwrap().unwrap(), b"v20".to_vec());
        assert_eq!(fdb_cp.get(b"k30").unwrap().unwrap(), b"v30".to_vec());
        assert_eq!(fdb_cp.get(b"k40").unwrap().unwrap(), b"v40".to_vec());

        // verify aux
        assert_eq!(fdb_cp.get_aux(b"k11").unwrap().unwrap(), b"v11".to_vec());
        assert_eq!(fdb_cp.get_aux(b"k21").unwrap().unwrap(), b"v21".to_vec());
        assert_eq!(fdb_cp.get_aux(b"k31").unwrap().unwrap(), b"v31".to_vec());

        // iterate data on range ["k10", "k40")
        let iter = fdb_cp.iter(b"k10", b"k40", IterOrder::Desc);
        let expected = vec![
            (b"k30".to_vec(), b"v30".to_vec()),
            (b"k20".to_vec(), b"v20".to_vec()),
            (b"k10".to_vec(), b"v10".to_vec()),
        ];
        let actual = iter
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected, actual);

        // iterate aux on range ["k11", "k31")
        let iter_aux = fdb_cp.iter_aux(b"k11", b"k31", IterOrder::Desc);
        let expected_aux = vec![
            (b"k21".to_vec(), b"v21".to_vec()),
            (b"k11".to_vec(), b"v11".to_vec()),
        ];
        let actual_aux = iter_aux
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(expected_aux, actual_aux);
    }
}
