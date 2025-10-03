package io.trailbase.examples.recordApi

import kotlinx.serialization.Serializable

@Serializable data class SimpleStrict(val id: String, val text_not_null: String)

@Serializable data class SimpleStrictInsert(val text_not_null: String)

@Serializable data class SimpleStrictUpdate(val text_not_null: String?)
