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
    var id: Int? = null,
    var name: String,
    var age: Int?,
)
```

You can now access database table and views by using `Client::records`.
This returns a `RecordApi` which supports all CRUD operations.
```kotlin
val myRecordApi = client.records("<TABLE_NAME>")

// get all persons
val persons = myRecordApi.list<MyPerson>().records

// create new person
val person = MyPerson(name = "foo", age = null)
val newPersonId = myRecordApi.create(person)

// update existing person
person.age = 42
myRecordApi.update(newPersonId, person)

// delete person
myRecordApi.delete(newPersonId)
```

## Kotlinx Serialization tradeoffs
To avoid issues, please always make sure that your Kotlin classes are as close as possible to the database scheme.
For example, you can use [online tools](https://github.com/wuseal/JsonToKotlinClass) to automatically generate Kotlin classes from the database's JSON schema.

By default, the Kotlin client does not send the value of fields that are `null`. For example, in the above example in the `myRecordApi.create(person)` call above, the `id` and `age` fields are not sent to the server. As a result, the Trailbase server uses the default values for the fields (e.g. automatically generated an `id`).
That behavior has various advantages, but also comes with some caveats. See [here](https://github.com/trailbaseio/trailbase/pull/287) for the discussion about it.

### Updating columns to a `null` value

In some cases, you do want to explicitly send fields with `null` as value, for example to update a record and set one if its fields to `null`. In that case, you have to use a custom `kotlinx` serializer that distincts between `null` and unset (i.e. not sending a value so that it remains unchanged). For example, the [Omittable library](https://github.com/Osmerion/Omittable) provides one. To use it, wrap the field's type into the generic `Omittable` type and annotate field with `@EncodeDefaults(EncodeDefault.Mode.NEVER)`.

```kotlin
@Serializable
data class MyPerson(
    var id: Int? = null,
    var name: String? = null,
    @EncodeDefaults(EncodeDefault.Mode.NEVER)
    var age: Omittable<Int?> = Omittable.absent(),
)

// sets the age to `null`
myRecordApi.update(newPersonId, Person(age = Omittable.of(null)))
```
