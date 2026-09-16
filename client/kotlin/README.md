# TrailBase Kotlin client

[TrailBase](https://trailbase.io) is an open, [sub-millisecond](https://trailbase.io/reference/benchmarks/), single-executable Firebase alternative with type-safe APIs, built-in WebAssembly runtime, realtime, auth, admin UI, ... built on Rust & SQLite (PG exp).

You can use this first-party multi-platform client to hook up your Kotlin applications: mobile, desktop and server.
More examples can be found [here](https://github.com/trailbaseio/trailbase/tree/main/client/kotlin/examples/record_api).

## Quick start

To get started, connect a client and sign-in:

```kotlin
val client = Client("https://mydomain.org:4000")
client.login(username, password)
```

You can also use `Client::register()` to create a new account first, which depending on your setup may send a email address verification email.

### Accessing records

To access data (a.k.a. records) in your database tables or views via configured APIs, you can either loosely work with `JsonObject`s or let the client automatically (de)serialize records from and to Kotlin objects.
For the latter, you'll need to provide the language bindings, either by generating them from JSON schemas or by rolling your own data classes annotated with [`@Serializable`](https://github.com/kotlin/kotlinx.serialization), e.g.:

```kotlin
@Serializable
data class Person(
    var id: Int? = null,
    var name: String,
    var age: Int?,
)

// access the "people" API:
val people = client.records("people")

// get everyone:
val persons = people.list<Person>().records

// create a new entry and read it back:
val person = Person(name = "foo", age = null)
val id = people.create(person)
val personFromDb = people.read(id)
```

Note that the schema requirements for reads, inserts and updates may all differ.
Above, we required `Person` to have a name but updates are differential allowing changes to individual fields:

```kotlin
@Serializable
data class PersonUpdate(
    var id: Int? = null,
    var name: String? = null,
    var age: Int? = null,
)

// update an existing person:
people.update(id, PersonUpdate(age = 42))

// alternatively untyped:
// people.update(id, buildJsonObject { put("age", 42) })

// and finally clean up:
people.delete(id)
```

## Strict-typing trade-offs

Whenever you strictly type database records, you need to be mindful of skew between your clients and your server: columns may be altered, removed or added on a schedule independent from your client rollouts. This is especially true for applications that run on users' devices where you have limited control like mobile or desktop.

Looking back at the example above, imagine the `name` column was removed. W/o changing the code first and rolling the changes out consistently, serialization will start failing.

If you have full control over your release cycle, you can consider generating strong types from JSON schema, e.g. using tools like [this](https://github.com/wuseal/JsonToKotlinClass). Alternatively, consider a looser [protobuf-like](https://protobuf.dev/best-practices/dos-donts/#add-required) approach where all fields should be consider optional, e.g.:

```kotlin
@Serializable
data class Person(
    var id: Int? = null,
    var name: String? = null,
    var age: Int? = null,
)
```

### Overriding fields with `null` values

The serialization is set up to skip fields that are `null`.[^1]
For example, the `people.create()` call does not put the `id` and `age` fields on the wire.
Instead, the server uses table defaults to derive these values like auto-generating the `id`.

While this may be a sensible default in most situations, sometimes you may want to explicitly write a `null` value, e.g.:

- to null a field of an existing record, or
- to create a record with a null value for a column with a non-null default.

If so, you can use a wrapper like [Omittable](https://github.com/Osmerion/Omittable) to explicitly distinguish between `null` and absent.
Simply wrap your field and annotate it with `@EncodeDefaults(EncodeDefault.Mode.NEVER)`:

```kotlin
@Serializable
data class Person(
    var id: Int? = null,
    var name: String? = null,

    @EncodeDefaults(EncodeDefault.Mode.NEVER)
    var age: Omittable<Int?> = Omittable.absent(),
)

// sets the age to `null`
people.update(id, Person(age = Omittable.Present(null)))
```

[^1]: This does not apply when working with `JsonObject`s directly.
