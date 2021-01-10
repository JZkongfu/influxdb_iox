#![deny(rust_2018_idioms)]
#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]
#![allow(unused_variables)]
pub mod chunk;
pub mod column;
pub mod row_group;
pub(crate) mod table;

use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use arrow_deps::arrow::{
    array::{ArrayRef, StringArray},
    datatypes::{DataType::Utf8, Field, Schema},
    record_batch::RecordBatch,
};
use snafu::{OptionExt, ResultExt, Snafu};

use chunk::Chunk;
use column::AggregateType;
pub use column::{FIELD_COLUMN_TYPE, TAG_COLUMN_TYPE, TIME_COLUMN_TYPE};
use row_group::{ColumnName, Predicate, RowGroup};
use table::Table;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("arrow conversion error: {}", source))]
    ArrowError {
        source: arrow_deps::arrow::error::ArrowError,
    },

    #[snafu(display("partition key does not exist: {}", key))]
    PartitionNotFound { key: String },

    #[snafu(display("chunk id does not exist: {}", id))]
    ChunkNotFound { id: u32 },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

// A database is scoped to a single tenant. Within a database there exists
// partitions, chunks, tables and row groups.
#[derive(Default)]
pub struct Database {
    // The collection of partitions for the database. Each partition is uniquely
    // identified by a partition key
    partitions: BTreeMap<String, Partition>,

    // The current total size of the database.
    size: u64,

    // Total number of rows in the database.
    rows: u64,
}

impl Database {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lists all chunks available in the specified partition
    pub fn chunks(&self, partition_key: &str) -> Vec<Arc<Chunk>> {
        self.partitions
            .get(partition_key)
            .map(|p| p.chunks())
            .unwrap_or_else(|| Vec::new())
    }

    /// Adds new data for a chunk.
    ///
    /// Data should be provided as a single row group for a table within the
    /// chunk. If the `Table` or `Chunk` does not exist they will be created,
    /// otherwise relevant structures will be updated.
    pub fn upsert_partition(
        &mut self,
        partition_key: &str,
        chunk_id: u32,
        table_name: &str,
        table_data: RecordBatch,
    ) {
        // validate table data contains appropriate meta data.
        let schema = table_data.schema();
        if schema.fields().len() != schema.metadata().len() {
            todo!("return error with missing column types for fields")
        }

        // ensure that a valid column type is specified for each column in
        // the record batch.
        for (col_name, col_type) in schema.metadata() {
            match col_type.as_str() {
                TAG_COLUMN_TYPE | FIELD_COLUMN_TYPE | TIME_COLUMN_TYPE => continue,
                _ => todo!("return error with incorrect column type specified"),
            }
        }

        let row_group = RowGroup::from(table_data);
        self.size += row_group.size();
        self.rows += row_group.rows() as u64;

        // create a new chunk if one doesn't exist, or add the table data to
        // the existing chunk.
        match self.partitions.entry(partition_key.to_owned()) {
            Entry::Occupied(mut e) => {
                let partition = e.get_mut();
                partition.upsert_chunk(chunk_id, table_name, row_group);
            }
            Entry::Vacant(e) => {
                let table = Table::new(table_name.into(), row_group);
                e.insert(Partition::new(partition_key, Chunk::new(chunk_id, table)));
            }
        };
    }

    /// Remove all row groups, tables and chunks within the specified partition
    /// key.
    pub fn drop_partition(&mut self, partition_key: &str) -> Result<()> {
        if self.partitions.remove(partition_key).is_some() {
            return Ok(());
        }

        Err(Error::PartitionNotFound {
            key: partition_key.to_owned(),
        })
    }

    /// Remove all row groups and tables for the specified chunks and partition.
    pub fn drop_chunk(&mut self, partition_key: &str, chunk_id: u32) -> Result<Arc<Chunk>> {
        let partition = self
            .partitions
            .get_mut(partition_key)
            .ok_or(Error::PartitionNotFound {
                key: partition_key.to_owned(),
            })?;

        partition
            .chunks
            .remove(&chunk_id)
            .context(ChunkNotFound { id: chunk_id })
    }

    /// Get a chunk by id
    pub fn get_chunk(&self, partition_key: &str, chunk_id: u32) -> Result<Arc<Chunk>> {
        self.partitions
            .get(partition_key)
            .ok_or(Error::PartitionNotFound {
                key: partition_key.to_owned(),
            })
            .and_then(|p| p.get_chunk(chunk_id))
    }

    // Lists all partition keys with data for this database.
    pub fn partition_keys(&mut self) -> Vec<&String> {
        self.partitions.keys().collect::<Vec<_>>()
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn rows(&self) -> u64 {
        self.rows
    }

    /// Determines the total number of tables under all partitions within the
    /// database.
    pub fn tables(&self) -> usize {
        self.partitions
            .values()
            .map(|partition| partition.tables())
            .sum()
    }

    /// Determines the total number of row groups under all tables under all
    /// chunks, within the database.
    pub fn row_groups(&self) -> usize {
        self.partitions
            .values()
            .map(|chunk| chunk.row_groups())
            .sum()
    }

    /// Executes selections against matching chunks, returning a single
    /// record batch with all chunk results appended.
    ///
    /// Results may be filtered by (currently only) equality predicates, but can
    /// be ranged by time, which should be represented as nanoseconds since the
    /// epoch. Results are included if they satisfy the predicate and fall
    /// with the [min, max) time range domain.
    pub fn select(
        &self,
        table_name: &str,
        time_range: (i64, i64),
        predicates: &[Predicate<'_>],
        select_columns: Vec<String>,
    ) -> Option<RecordBatch> {
        // Find all matching chunks using:
        //   - time range
        //   - measurement name.
        //
        // Execute against each chunk and append each result set into a
        // single record batch.
        todo!();
    }

    /// Returns aggregates segmented by grouping keys for the specified
    /// measurement as record batches, with one record batch per matching
    /// chunk.
    ///
    /// The set of data to be aggregated may be filtered by (currently only)
    /// equality predicates, but can be ranged by time, which should be
    /// represented as nanoseconds since the epoch. Results are included if they
    /// satisfy the predicate and fall with the [min, max) time range domain.
    ///
    /// Group keys are determined according to the provided group column names.
    /// Currently only grouping by string (tag key) columns is supported.
    ///
    /// Required aggregates are specified via a tuple comprising a column name
    /// and the type of aggregation required. Multiple aggregations can be
    /// applied to the same column.
    pub fn aggregate(
        &self,
        table_name: &str,
        time_range: (i64, i64),
        predicates: &[Predicate<'_>],
        group_columns: Vec<String>,
        aggregates: Vec<(ColumnName<'_>, AggregateType)>,
    ) -> Option<RecordBatch> {
        // Find all matching chunks using:
        //   - time range
        //   - measurement name.
        //
        // Execute query against each matching chunk and get result set.
        // For each result set it may be possible for there to be duplicate
        // group keys, e.g., due to back-filling. So chunk results may need
        // to be merged together with the aggregates from identical group keys
        // being resolved.
        //
        // Finally a record batch is returned.
        todo!()
    }

    /// Returns aggregates segmented by grouping keys and windowed by time.
    ///
    /// The set of data to be aggregated may be filtered by (currently only)
    /// equality predicates, but can be ranged by time, which should be
    /// represented as nanoseconds since the epoch. Results are included if they
    /// satisfy the predicate and fall with the [min, max) time range domain.
    ///
    /// Group keys are determined according to the provided group column names
    /// (`group_columns`). Currently only grouping by string (tag key) columns
    /// is supported.
    ///
    /// Required aggregates are specified via a tuple comprising a column name
    /// and the type of aggregation required. Multiple aggregations can be
    /// applied to the same column.
    ///
    /// Results are grouped and windowed according to the `window` parameter,
    /// which represents an interval in nanoseconds. For example, to window
    /// results by one minute, window should be set to 600_000_000_000.
    pub fn aggregate_window(
        &self,
        table_name: &str,
        time_range: (i64, i64),
        predicates: &[Predicate<'_>],
        group_columns: Vec<String>,
        aggregates: Vec<(ColumnName<'_>, AggregateType)>,
        window: i64,
    ) -> Option<RecordBatch> {
        // Find all matching chunks using:
        //   - time range
        //   - measurement name.
        //
        // Execute query against each matching chunk and get result set.
        // For each result set it may be possible for there to be duplicate
        // group keys, e.g., due to back-filling. So chunk results may need
        // to be merged together with the aggregates from identical group keys
        // being resolved.
        //
        // Finally a record batch is returned.
        todo!()
    }

    //
    // ---- Schema API queries
    //

    /// Returns the distinct set of table names that contain data that satisfies
    /// the time range and predicates.
    ///
    /// TODO(edd): Implement predicate support.
    pub fn table_names(
        &self,
        partition_key: &str,
        chunk_ids: &[u32],
        predicates: &[Predicate<'_>],
    ) -> Result<Option<RecordBatch>> {
        let partition = self
            .partitions
            .get(partition_key)
            .ok_or(Error::PartitionNotFound {
                key: partition_key.to_owned(),
            })?;

        let mut intersection = BTreeSet::new();
        let chunk_table_names = partition
            .chunks
            .values()
            .map(|chunk| chunk.table_names(predicates))
            .for_each(|mut names| intersection.append(&mut names));

        if intersection.is_empty() {
            return Ok(None);
        }

        let schema = Schema::new(vec![Field::new("table", Utf8, false)]);
        let columns: Vec<ArrayRef> = vec![Arc::new(StringArray::from(
            intersection
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>(),
        ))];

        match RecordBatch::try_new(Arc::new(schema), columns).context(ArrowError {}) {
            Ok(rb) => Ok(Some(rb)),
            Err(e) => Err(e),
        }
    }

    /// Returns the distinct set of tag keys (column names) matching the
    /// provided optional predicates and time range.
    pub fn tag_keys(
        &self,
        table_name: &str,
        time_range: (i64, i64),
        predicates: &[Predicate<'_>],
    ) -> Option<RecordBatch> {
        // Find all matching chunks using:
        //   - time range
        //   - measurement name.
        //
        // Execute query against matching chunks. The `tag_keys` method for
        // a chunk allows the caller to provide already found tag keys
        // (column names). This allows the execution to skip entire chunks,
        // tables or segments if there are no new columns to be found there...
        todo!();
    }

    /// Returns the distinct set of tag values (column values) for each provided
    /// tag key, where each returned value lives in a row matching the provided
    /// optional predicates and time range.
    ///
    /// As a special case, if `tag_keys` is empty then all distinct values for
    /// all columns (tag keys) are returned for the chunk.
    pub fn tag_values(
        &self,
        table_name: &str,
        time_range: (i64, i64),
        predicates: &[Predicate<'_>],
        tag_keys: &[String],
    ) -> Option<RecordBatch> {
        // Find the measurement name on the chunk and dispatch query to the
        // table for that measurement if the chunk's time range overlaps the
        // requested time range.
        todo!();
    }
}

impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database")
            .field("partitions", &self.partitions.keys())
            .field("size", &self.size)
            .finish()
    }
}

// A partition is a collection of `Chunks`.
#[derive(Default)]
pub struct Partition {
    // The partition's key
    key: String,

    // The collection of chunks in the partition. Each chunk is uniquely
    // identified by a chunk id.
    chunks: BTreeMap<u32, Arc<Chunk>>,

    // The current total size of the partition.
    size: u64,

    // Total number of rows in the partition.
    rows: u64,
}

impl Partition {
    pub fn new(partition_key: &str, chunk: Chunk) -> Self {
        let mut p = Self {
            key: partition_key.to_owned(),
            size: chunk.size(),
            rows: chunk.rows(),
            chunks: BTreeMap::new(),
        };
        p.chunks.insert(chunk.id(), Arc::new(chunk));
        p
    }

    /// Lists all chunks available in this partition
    pub fn chunks(&self) -> Vec<Arc<Chunk>> {
        self.chunks
            .iter()
            .map(|(_, chunk)| chunk.clone())
            .collect::<Vec<_>>()
    }

    pub fn get_chunk(&self, chunk_id: u32) -> Result<Arc<Chunk>> {
        self.chunks
            .get(&chunk_id)
            .map(|c| c.clone())
            .context(ChunkNotFound { id: chunk_id })
    }

    /// Adds new data for a chunk.
    ///
    /// Data should be provided as a single row group for a table within the
    /// chunk. If the `Table` or `Chunk` does not exist they will be created,
    /// otherwise relevant structures will be updated.
    fn upsert_chunk(&mut self, chunk_id: u32, table_name: &str, row_group: RowGroup) {
        self.size += row_group.size();
        self.rows += row_group.rows() as u64;

        // create a new chunk if one doesn't exist, or add the table data to
        // the existing chunk.
        match self.chunks.entry(chunk_id) {
            Entry::Occupied(mut e) => {
                let mut chunk = e.get_mut();
                // TODO this needs to get fixed so we don't have to
                // mutate the chunk but rather can create it in one
                // shot perhaps by allowing callers to create the
                // chunk and pass it in rather than passing in row
                // groups..
                let chunk = Arc::get_mut(&mut chunk).unwrap();
                chunk.upsert_table(table_name, row_group);
            }
            Entry::Vacant(e) => {
                let chunk = Chunk::new(chunk_id, Table::new(table_name.into(), row_group));
                e.insert(Arc::new(chunk));
            }
        };
    }

    /// Determines the total number of tables under all chunks within the
    /// partition.
    pub fn tables(&self) -> usize {
        self.chunks.values().map(|chunk| chunk.tables()).sum()
    }

    /// Determines the total number of row groups under all tables under all
    /// chunks, within the partition.
    pub fn row_groups(&self) -> usize {
        self.chunks.values().map(|chunk| chunk.row_groups()).sum()
    }

    pub fn rows(&self) -> u64 {
        self.rows
    }
}

/// Generate a predicate for the time range [from, to).
pub fn time_range_predicate<'a>(from: i64, to: i64) -> Vec<row_group::Predicate<'a>> {
    vec![
        (
            row_group::TIME_COLUMN_NAME,
            (
                column::cmp::Operator::GTE,
                column::Value::Scalar(column::Scalar::I64(from)),
            ),
        ),
        (
            row_group::TIME_COLUMN_NAME,
            (
                column::cmp::Operator::LT,
                column::Value::Scalar(column::Scalar::I64(to)),
            ),
        ),
    ]
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::sync::Arc;

    use arrow_deps::arrow::{
        array::{ArrayRef, Float64Array, Int64Array, StringArray},
        datatypes::{
            DataType::{Float64, Int64, Utf8},
            Field, Schema,
        },
    };

    use super::*;

    // helper to make the `database_update_chunk` test simpler to read.
    fn gen_recordbatch() -> RecordBatch {
        let metadata = vec![
            ("region".to_owned(), TAG_COLUMN_TYPE.to_owned()),
            ("counter".to_owned(), FIELD_COLUMN_TYPE.to_owned()),
            (
                row_group::TIME_COLUMN_NAME.to_owned(),
                TIME_COLUMN_TYPE.to_owned(),
            ),
        ]
        .into_iter()
        .collect::<HashMap<String, String>>();

        let schema = Schema::new_with_metadata(
            vec![
                ("region", Utf8),
                ("counter", Float64),
                (row_group::TIME_COLUMN_NAME, Int64),
            ]
            .into_iter()
            .map(|(name, typ)| Field::new(name, typ, false))
            .collect(),
            metadata,
        );

        let data: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["west", "west", "east"])),
            Arc::new(Float64Array::from(vec![1.2, 3.3, 45.3])),
            Arc::new(Int64Array::from(vec![11111111, 222222, 3333])),
        ];

        RecordBatch::try_new(Arc::new(schema), data).unwrap()
    }

    #[test]
    fn database_update_partition() {
        let mut db = Database::new();
        db.upsert_partition("hour_1", 22, "a_table", gen_recordbatch());

        assert_eq!(db.rows(), 3);
        assert_eq!(db.tables(), 1);
        assert_eq!(db.row_groups(), 1);

        let partition = db.partitions.values().next().unwrap();
        assert_eq!(partition.tables(), 1);
        assert_eq!(partition.rows(), 3);
        assert_eq!(partition.row_groups(), 1);

        // Updating the chunk with another row group for the table just adds
        // that row group to the existing table.
        db.upsert_partition("hour_1", 22, "a_table", gen_recordbatch());
        assert_eq!(db.rows(), 6);
        assert_eq!(db.tables(), 1); // still one table
        assert_eq!(db.row_groups(), 2);

        let partition = db.partitions.values().next().unwrap();
        assert_eq!(partition.tables(), 1); // it's the same table.
        assert_eq!(partition.rows(), 6);
        assert_eq!(partition.row_groups(), 2);

        // Adding the same data under another table would increase the table
        // count.
        db.upsert_partition("hour_1", 22, "b_table", gen_recordbatch());
        assert_eq!(db.rows(), 9);
        assert_eq!(db.tables(), 2);
        assert_eq!(db.row_groups(), 3);

        let partition = db.partitions.values().next().unwrap();
        assert_eq!(partition.tables(), 2);
        assert_eq!(partition.rows(), 9);
        assert_eq!(partition.row_groups(), 3);

        // Adding the data under another chunk adds a new chunk.
        db.upsert_partition("hour_1", 29, "a_table", gen_recordbatch());
        assert_eq!(db.rows(), 12);
        assert_eq!(db.tables(), 3); // two distinct tables but across two chunks.
        assert_eq!(db.row_groups(), 4);

        let partition = db.partitions.values().next().unwrap();
        assert_eq!(partition.tables(), 3);
        assert_eq!(partition.rows(), 12);
        assert_eq!(partition.row_groups(), 4);

        let chunk_22 = db
            .partitions
            .get("hour_1")
            .unwrap()
            .chunks
            .values()
            .next()
            .unwrap();
        assert_eq!(chunk_22.tables(), 2);
        assert_eq!(chunk_22.rows(), 9);
        assert_eq!(chunk_22.row_groups(), 3);

        let chunk_29 = db
            .partitions
            .get("hour_1")
            .unwrap()
            .chunks
            .values()
            .nth(1)
            .unwrap();
        assert_eq!(chunk_29.tables(), 1);
        assert_eq!(chunk_29.rows(), 3);
        assert_eq!(chunk_29.row_groups(), 1);
    }

    // Helper function to assert the contents of a column on a record batch.
    fn assert_rb_column_equals(rb: &RecordBatch, col_name: &str, exp: &column::Values<'_>) {
        let got_column = rb.column(rb.schema().index_of(col_name).unwrap());

        match exp {
            column::Values::String(exp_data) => {
                let arr: &StringArray = got_column.as_any().downcast_ref::<StringArray>().unwrap();
                assert_eq!(&arr.iter().collect::<Vec<_>>(), exp_data);
            }
            column::Values::I64(exp_data) => {
                let arr: &Int64Array = got_column.as_any().downcast_ref::<Int64Array>().unwrap();
                assert_eq!(arr.values(), exp_data);
            }
            column::Values::U64(_) => {}
            column::Values::F64(_) => {}
            column::Values::I64N(_) => {}
            column::Values::U64N(_) => {}
            column::Values::F64N(_) => {}
            column::Values::Bool(_) => {}
            column::Values::ByteArray(_) => {}
        }
    }

    #[test]
    fn table_names() {
        let mut db = Database::new();

        db.upsert_partition("hour_1", 22, "Coolverine", gen_recordbatch());
        let data = db.table_names("hour_1", &[22], &[]).unwrap().unwrap();
        assert_rb_column_equals(
            &data,
            "table",
            &column::Values::String(vec![Some("Coolverine")]),
        );

        db.upsert_partition("hour_1", 22, "Coolverine", gen_recordbatch());
        let data = db.table_names("hour_1", &[22], &[]).unwrap().unwrap();
        assert_rb_column_equals(
            &data,
            "table",
            &column::Values::String(vec![Some("Coolverine")]),
        );

        db.upsert_partition("hour_1", 2, "Coolverine", gen_recordbatch());
        let data = db.table_names("hour_1", &[22], &[]).unwrap().unwrap();
        assert_rb_column_equals(
            &data,
            "table",
            &column::Values::String(vec![Some("Coolverine")]),
        );

        db.upsert_partition("hour_1", 2, "20 Size", gen_recordbatch());
        let data = db.table_names("hour_1", &[22], &[]).unwrap().unwrap();
        assert_rb_column_equals(
            &data,
            "table",
            &column::Values::String(vec![Some("20 Size"), Some("Coolverine")]),
        );
    }
}
