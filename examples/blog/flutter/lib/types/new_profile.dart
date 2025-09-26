// To parse this JSON data, do
//
//     final newProfile = newProfileFromJson(jsonString);

import 'dart:convert';

NewProfile newProfileFromJson(String str) =>
    NewProfile.fromJson(json.decode(str));

String newProfileToJson(NewProfile data) => json.encode(data.toJson());

class NewProfile {
  int? created;
  int? updated;
  String? user;
  String username;

  NewProfile({
    this.created,
    this.updated,
    this.user,
    required this.username,
  });

  factory NewProfile.fromJson(Map<String, dynamic> json) => NewProfile(
        created: json["created"],
        updated: json["updated"],
        user: json["user"],
        username: json["username"],
      );

  Map<String, dynamic> toJson() => {
        "created": created,
        "updated": updated,
        "user": user,
        "username": username,
      };
}
