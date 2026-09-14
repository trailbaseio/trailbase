# TrailBase Kotlin client

This is the first-party client for hooking up your Kotlin applications
with TrailBase.

You can find an example how to use the Kotlin client in under [`client/kotlin/examples/record_api`](https://github.com/trailbaseio/trailbase/tree/main/client/kotlin/examples/record_api).

## Client Setup and Authentication 
To get started, construct a TrailBase client
```kotlin
val client = Client()
```

You can use `Client::register` for creating new accounts. To log in with the user's credentials,
use `Client::login` and the other login-related methods.

```kotlin
client.login(username, password)
```

## Accessing records
The TrailBase client can automatically deserialize the record API's JSON responses to Kotlin object.
To do so, the Kotlin classes must be annotated with `@Serializable` from [`kotlinx.serialization`](https://github.com/kotlin/kotlinx.serialization).

For example, this would look like
```kotlin
@Serializable
data class MyPerson(
    var id: Int?,
    var name: String,
    var age: Int? = null,
)
```

You can now access database table and views by using `Client::records`.
This returns a `RecordApi` which supports all CRUD operations.
```kotlin
val myRecordApi = client.records("<TABLE_NAME>")

// get all persons
val persons = myRecordApi.list<MyPerson>().records

// create new person
val person = MyPerson(null, "foo")
val newPersonId = myRecordApi.create().records

// update existing person
person.age = 42
myRecordApi.update(newPersonId, person)

// delete person
myRecordApi.delete(newPersonId)
```

## Kotlinx Serialization tradeoffs
To avoid issues, please always make sure that your Kotlin classes are as close as possible to the database scheme.
For example, you can use [online tools](https://www.schemato.top/sql-to-kotlin) to automatically generate Kotlin classes from the SQL table schema.

For more information, see the discussion [here](https://github.com/trailbaseio/trailbase/pull/287).
