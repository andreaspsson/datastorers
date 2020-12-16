mod connection;
use crate::connection::{create_test_connection};
use datastore_entity::{DatastoreManaged, DatastoreClientError, DatastoreParseError, DatastorersError};
use datastore_entity::transaction::{TransactionConnection};
use google_datastore1::schemas::Key;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;

use std::convert::TryInto;

#[derive(DatastoreManaged, Clone, Debug)]
#[kind = "Test"]
#[page_size = 2]
pub struct TestEntity {
    #[key]
    pub key: Option<Key>,

    #[version]
    pub version: Option<i64>,

    #[indexed]
    #[property = "Name"]
    pub prop_string: String,

    #[property = "bool_property"]
    pub prop_bool: bool,

    #[indexed]
    #[property = "int_property"]
    pub prop_int: i64,

    #[indexed]
    #[property = "str_array_property"]
    pub prop_string_array: Vec<String>,
}

#[derive(DatastoreManaged, Clone, Debug)]
#[kind = "Test"]
#[page_size = 2]
pub struct TestEntityOptional {
    #[key]
    pub key: Option<Key>,

    #[indexed]
    #[property = "Name"]
    pub prop_string: Option<String>,

    #[property = "bool_property"]
    pub prop_bool: Option<bool>,

    #[property = "int_property"]
    pub prop_int: Option<i64>,

    #[property = "str_array_property"]
    pub prop_string_array: Option<Vec<String>>,
}

impl Default for TestEntityOptional {
    fn default() -> Self {
        TestEntityOptional {
            key: None,
            prop_string: None,
            prop_bool: None,
            prop_int: None,
            prop_string_array: None,
        }
    }
}

fn generate_random_string(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .collect()
}

fn generate_random_bool() -> bool {
    let val = thread_rng().gen_range(0, 2);
    val != 0
}

fn generate_random_int() -> i64 {
    thread_rng().gen()   
}

fn generate_random_entity() -> TestEntity {
    TestEntity {
        key: None,
        version: None,
        prop_string: generate_random_string(10),
        prop_bool: generate_random_bool(),
        prop_int: generate_random_int(),
        prop_string_array: vec![],
    }
}

fn assert_client_error<T>(result: Result<T, DatastorersError>, expected_error: DatastoreClientError) {
    match result {
        Ok(_) => panic!("expect no entity to be found"),
        Err(e) => match e {
            DatastorersError::DatastoreClientError(client_error) =>
                assert_eq!(client_error, expected_error, "Expected error to be {}", expected_error),
            _ => panic!("Expected DatastoreClientError"),
        },
    };
}

fn assert_parse_error<T>(result: Result<T, DatastorersError>, expected_error: DatastoreParseError) {
    match result {
        Ok(_) => panic!("expect no entity to be found"),
        Err(e) => match e {
            DatastorersError::ParseError(parse_error) =>
                assert_eq!(parse_error, expected_error, "Expected error to be {}", expected_error),
            _ => panic!("Expected DatastoreParseError"),
        },
    };
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_insert_and_update() -> Result<(), DatastorersError> {
    let connection = create_test_connection();
    let original_entity = generate_random_entity();
    let original_bool_value = original_entity.prop_bool;

    let mut test_entity = original_entity.commit(&connection)?;
    assert!(test_entity.key.is_some());
    // Save id for later validations
    let id_after_insert = test_entity.id().unwrap().clone();

    // Update same item
    test_entity.prop_bool = !original_bool_value;
    let updated = test_entity.commit(&connection)?;

    // Shall have been updated
    assert_eq!(updated.prop_bool, !original_bool_value);

    // But id shall remain the same
    assert_eq!(&id_after_insert, updated.id().unwrap());

    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_get_by_id() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    // Insert an entity with some random values
    let entity = generate_random_entity();
    let original_string = entity.prop_string.clone();
    let original_int = entity.prop_int.clone();
    
    let inserted = entity.commit(&connection)?;

    // Try fetch with a random id, to validate that not found check works
    let random_id = generate_random_int();
    assert_client_error(
        TestEntity::get_one_by_id(random_id, &connection),
        DatastoreClientError::NotFound
    );

    // Success
    let inserted_id = inserted.key.unwrap().path.unwrap()[0].id.unwrap();
    let fetched_entity = TestEntity::get_one_by_id(inserted_id, &connection)?;

    // Validate content of the fetched entity
    assert_eq!(&original_string, &fetched_entity.prop_string);
    assert_eq!(original_int, fetched_entity.prop_int);

    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_get_by_property() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    // Save 3 entities, 2 with the same name
    let expected_result_entity = generate_random_entity().commit(&connection)?;
    let duplicated_entity = generate_random_entity();
    duplicated_entity.clone().commit(&connection)?;
    duplicated_entity.clone().commit(&connection)?;

    assert_ne!(expected_result_entity.prop_int, duplicated_entity.prop_int);

    // Not found
    assert_client_error(
        TestEntity::get_one_by_prop_string(generate_random_string(10), &connection),
        DatastoreClientError::NotFound
    );

    // Multiple results
    assert_client_error(
        TestEntity::get_one_by_prop_string(duplicated_entity.prop_string, &connection),
        DatastoreClientError::AmbiguousResult
    );

    // Success
    let fetched_entity = match TestEntity::get_one_by_prop_string(expected_result_entity.prop_string, &connection) {
        Ok(e) => e,
        Err(e) => panic!("Failed to fetch entity: {}", e),
    };
    assert_eq!(fetched_entity.prop_int, expected_result_entity.prop_int);

    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_get_collection_by_property() -> Result<(), DatastorersError> {
    let page_size = 2;
    let connection = create_test_connection();

    // Create some entities (5 of them, that is enough for this test since page size in test build is 2)
    let common_string_prop = generate_random_string(15);
    let mut int_props = vec![]; // Save all upserted int props so we can validate the result later
    let mut fetched_int_props = vec![];
    for _ in 0..5 {
        let mut entity = generate_random_entity();
        entity.prop_string = common_string_prop.clone();
        let inserted = entity.commit(&connection)?;
        int_props.push(inserted.prop_int);
    }

    // Fetch first page
    let page = TestEntity::get_by_prop_string(common_string_prop, &connection)?;

    // Validate it
    assert_eq!(page.result.len(), page_size);
    assert!(page.has_more_results);
    for val in page.result.iter() {
        fetched_int_props.push(val.prop_int);
    }

    // Fetch next page
    let page_two = page.get_next_page(&connection)?;

    // Validate it
    assert_eq!(page_two.result.len(), page_size);
    assert!(page_two.has_more_results);
    for val in page_two.result.iter() {
        fetched_int_props.push(val.prop_int);
    }

    // Fetch last page
    let last_page = page_two.get_next_page(&connection)?;

    assert_eq!(last_page.result.len(), 1); // Shall now only be one item!
    assert!(!last_page.has_more_results); // Shall now not have any more results!
    for val in last_page.result.iter() {
        fetched_int_props.push(val.prop_int);
    }

    // Try to fetch one more page (shall fail)
    assert_client_error(
        last_page.get_next_page(&connection),
        DatastoreClientError::NoMorePages
    );

    // Compare the two int arrays to validate that all inserted items have been fetched
    assert_eq!(fetched_int_props.sort(), int_props.sort());

    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_update_property() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    // Create and insert
    let original = generate_random_entity();    
    let inserted = original.clone().commit(&connection)?;

    // Get by prop, shall be same key as created
    let mut fetched = TestEntity::get_one_by_prop_string(original.prop_string.clone(), &connection)?;
    assert_eq!(&inserted.key, &fetched.key);

    // Change the prop value and commit
    let new_string_prop = generate_random_string(10);
    fetched.prop_string = new_string_prop.clone();
    assert_ne!(&fetched.prop_string, &original.prop_string);
    fetched.commit(&connection)?;

    // Get by old prop value => not found
    assert_client_error(
        TestEntity::get_one_by_prop_string(original.prop_string.clone(), &connection),
        DatastoreClientError::NotFound
    );

    // Get by new prop value => entity shall be founfd, with the original key
    let fetched = TestEntity::get_one_by_prop_string(new_string_prop.clone(), &connection)?;
    assert_eq!(&inserted.key, &fetched.key);
    assert_eq!(&new_string_prop, &fetched.prop_string);

    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_get_by_array_property() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    // Generate some test entities
    let string_value_a = generate_random_string(10);
    let string_value_b = generate_random_string(10);
    let string_value_c = generate_random_string(10);

    let mut entity_a = generate_random_entity();
    entity_a.prop_string_array = vec![string_value_a.clone(), string_value_b.clone()];
    let mut entity_b = generate_random_entity();
    entity_b.prop_string_array = vec![string_value_b.clone(), string_value_c.clone()];

    // Insert
    let inserted_a = entity_a.commit(&connection)?;
    let inserted_b = entity_b.commit(&connection)?;

    // Fetch for string_value_a => shall return entity_a
    let fetched_entity = TestEntity::get_one_by_prop_string_array(string_value_a, &connection)?;
    assert_eq!(&inserted_a.key, &fetched_entity.key);


    // Fetch for string_value_c => shall return entity_b
    let fetched_entity = TestEntity::get_one_by_prop_string_array(string_value_c, &connection)?;
    assert_eq!(&inserted_b.key, &fetched_entity.key);

    // Fetch for string_value_b => shall return multiple entities => error
    assert_client_error(
        TestEntity::get_one_by_prop_string_array(string_value_b, &connection),
        DatastoreClientError::AmbiguousResult
    );

    Ok(())
}


#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_update_array_property() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    // Generate some test entities
    let string_value_a = generate_random_string(10);
    let string_value_b = generate_random_string(10);
    let string_value_c = generate_random_string(10);

    let mut entity = generate_random_entity();
    entity.prop_string_array = vec![string_value_a.clone(), string_value_b.clone()];
    
    // Insert
    let mut inserted = entity.commit(&connection)?;
    let inserted_key = inserted.key.clone();

    // Fetch for string_value_a => shall return entity
    let fetched_entity = TestEntity::get_one_by_prop_string_array(string_value_a.clone(), &connection)?;
    assert_eq!(&inserted.key, &fetched_entity.key);

    // Fetch for string_value_b => shall return entity
    let fetched_entity = TestEntity::get_one_by_prop_string_array(string_value_b.clone(), &connection)?;
    assert_eq!(&inserted.key, &fetched_entity.key);

    // Fetch for string_value_c => shall return error not found
    assert_client_error(
        TestEntity::get_one_by_prop_string_array(string_value_c.clone(), &connection),
        DatastoreClientError::NotFound
    );

    // Change array and commit
    inserted.prop_string_array.remove(0); // Remove string_value_a
    inserted.prop_string_array.push(string_value_c.clone()); // Push string_value_c
    inserted.commit(&connection)?;

    // Try fetch again, now a shall fail and b + c shall return the inserted entity
    
    // Fetch for string_value_a => shall return error not found
    assert_client_error(
        TestEntity::get_one_by_prop_string_array(string_value_a, &connection),
        DatastoreClientError::NotFound
    );

    // Fetch for string_value_b => shall return entity
    let fetched_entity = TestEntity::get_one_by_prop_string_array(string_value_b, &connection)?;
    assert_eq!(&inserted_key, &fetched_entity.key);

    // Fetch for string_value_c => shall return entity
    let fetched_entity = TestEntity::get_one_by_prop_string_array(string_value_c, &connection)?;
    assert_eq!(&inserted_key, &fetched_entity.key);

    Ok(())
}


#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_delete() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    let inserted_a = generate_random_entity().commit(&connection)?;
    let inserted_b = generate_random_entity().commit(&connection)?;

    // Both shall be fetchable
    let fetched = TestEntity::get_one_by_prop_string(inserted_a.prop_string.clone(), &connection)?;
    assert_eq!(&inserted_a.key, &fetched.key);
    let fetched = TestEntity::get_one_by_prop_string(inserted_b.prop_string.clone(), &connection)?;
    assert_eq!(&inserted_b.key, &fetched.key);


    // Delete one
    let prop_string_b = inserted_b.prop_string.clone();
    inserted_b.delete(&connection)?;

    // Only entity_a shall be fetchable
    let fetched = TestEntity::get_one_by_prop_string(inserted_a.prop_string.clone(), &connection)?;
    assert_eq!(&inserted_a.key, &fetched.key);

    assert_client_error(
        TestEntity::get_one_by_prop_string(prop_string_b, &connection),
        DatastoreClientError::NotFound
    );

    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_optional_values() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    let mut inserted_empty = TestEntityOptional::default().commit(&connection)?;
    let inserted_id = inserted_empty.clone().key.unwrap().path.unwrap()[0].id.unwrap();

    // Set string and bool value and commit
    let string_value = generate_random_string(10);
    inserted_empty.prop_string = Some(string_value.clone());
    inserted_empty.prop_bool = Some(true);
    inserted_empty.commit(&connection)?;
    
    // Fetch and validate that the inserted properties are saved
    let mut fetched_entity = TestEntityOptional::get_one_by_id(inserted_id, &connection)?;
    assert_eq!(&fetched_entity.prop_string, &Some(string_value.clone()));
    assert_eq!(&fetched_entity.prop_bool, &Some(true));
    assert_eq!(&fetched_entity.prop_int, &None);
    assert_eq!(&fetched_entity.prop_string_array, &None);

   // Try fetch with the non optional type, shalll fail since not all values are set!
   assert_parse_error(
       TestEntity::get_one_by_id(inserted_id, &connection),
       DatastoreParseError::NoSuchValue
   );

    // Set the rest of the values
    let int_value = generate_random_int();
    fetched_entity.prop_int = Some(int_value);
    fetched_entity.prop_string_array = Some(vec![]);
    fetched_entity.commit(&connection)?;

   // Fetch and validate result
   let fetched_entity = TestEntityOptional::get_one_by_prop_string(string_value.clone(), &connection)?;
   assert_eq!(&fetched_entity.prop_string, &Some(string_value.clone()));
   assert_eq!(&fetched_entity.prop_bool, &Some(true));
   assert_eq!(&fetched_entity.prop_int, &Some(int_value));
   assert_eq!(&fetched_entity.prop_string_array, &Some(vec![]));

   // Now shall it also be possible to fetch the entity type without optionals
   let fetched_non_optional = TestEntity::get_one_by_prop_int(int_value, &connection)?;
   assert_eq!(&fetched_non_optional.prop_string, &string_value);
   assert_eq!(&fetched_non_optional.prop_bool, &true);
   assert_eq!(&fetched_non_optional.prop_int, &int_value);
   let empty_vec: Vec<String> = vec![];
   assert_eq!(&fetched_non_optional.prop_string_array, &empty_vec);

    Ok(())
}


#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_coliding_update() -> Result<(), DatastorersError> {
    let connection = create_test_connection();
    // Insert one entity
    let inserted = generate_random_entity().commit(&connection)?;
    let inserted_id = inserted.key.unwrap().path.unwrap()[0].id.unwrap();

    // Go fetch it two times (and change both fetched entities)
    let mut a = TestEntity::get_one_by_id(inserted_id, &connection)?;
    let prop_int_a = generate_random_int();
    a.prop_int = prop_int_a;

    let mut b = TestEntity::get_one_by_id(inserted_id, &connection)?;
    b.prop_int = generate_random_int();

    // Save the forst one => we expect success
    a.commit(&connection)?;

    // Save the second one => we expect error (collision)
    assert_client_error(
        b.commit(&connection),
        DatastoreClientError::DataConflict
    );

    // Fetch one last time, the changes in a shall have been saved
    let fetched = TestEntity::get_one_by_id(inserted_id, &connection)?;
    assert_eq!(prop_int_a, fetched.prop_int);
    
    Ok(())
}

#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_coliding_delete() -> Result<(), DatastorersError> {
    let connection = create_test_connection();
    // Insert one entity
    let inserted = generate_random_entity().commit(&connection)?;
    let inserted_id = inserted.key.unwrap().path.unwrap()[0].id.unwrap();

    // Go fetch it two times (and change both fetched entities)
    let mut a = TestEntity::get_one_by_id(inserted_id, &connection)?;
    let prop_int_a = generate_random_int();
    a.prop_int = prop_int_a;

    let mut b = TestEntity::get_one_by_id(inserted_id, &connection)?;
    b.prop_int = generate_random_int();

    // Save the forst one => we expect success
    a.commit(&connection)?;

    // Delete the second one => we expect error (collision)
    assert_client_error(
        b.delete(&connection),
        DatastoreClientError::DataConflict
    );

    // Fetch one last time, the changes in a shall have been saved
    let fetched = TestEntity::get_one_by_id(inserted_id, &connection)?;
    assert_eq!(prop_int_a, fetched.prop_int);
    
    Ok(())
}


#[test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
fn test_transaction_with_update() -> Result<(), DatastorersError> {
    let connection = create_test_connection();

    // Create two entities
    let inserted = generate_random_entity().commit(&connection)?;
    let inserted_id_a = inserted.key.unwrap().path.unwrap()[0].id.unwrap();
    let original_prop_int_a = inserted.prop_int;
    let inserted = generate_random_entity().commit(&connection)?;
    let inserted_id_b = inserted.key.unwrap().path.unwrap()[0].id.unwrap();
    let original_prop_int_b = inserted.prop_int;

    // Create a transaction, use the transaction to fetch and modify both entities
    let mut transaction = TransactionConnection::begin_transaction(&connection)?;
    
    let mut a = TestEntity::get_one_by_id(inserted_id_a, &transaction)?;
    let prop_int_a = generate_random_int();
    a.prop_int = prop_int_a;
    transaction.push_save(a)?;

    let mut b = TestEntity::get_one_by_id(inserted_id_b, &transaction)?;
    let prop_int_b = generate_random_int();
    b.prop_int = prop_int_b;
    transaction.push_save(b)?;

    // Transaction not commited, a and b shall have their original values
    let fetched_a = TestEntity::get_one_by_id(inserted_id_a, &connection)?;
    assert_eq!(original_prop_int_a, fetched_a.prop_int);
    let fetched_b = TestEntity::get_one_by_id(inserted_id_b, &connection)?;
    assert_eq!(original_prop_int_b, fetched_b.prop_int);

    // Commit transaction
    transaction.commit()?;

    // Fetch and validate that both items got updated
    let fetched_a = TestEntity::get_one_by_id(inserted_id_a, &connection)?;
    assert_eq!(fetched_a.prop_int, prop_int_a);

    let fetched_b = TestEntity::get_one_by_id(inserted_id_b, &connection)?;
    assert_eq!(fetched_b.prop_int, prop_int_b);

    Ok(())
}
