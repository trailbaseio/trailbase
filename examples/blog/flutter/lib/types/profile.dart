// To parse this JSON data, do
//
//     final profile = profileFromJson(jsonString);

import 'dart:convert';

Profile profileFromJson(String str) => Profile.fromJson(json.decode(str));

String profileToJson(Profile data) => json.encode(data.toJson());

class Profile {
  String? avatarUrl;
  int created;
  bool? isEditor;
  int updated;
  String user;
  String username;

  Profile({
    this.avatarUrl,
    required this.created,
    this.isEditor,
    required this.updated,
    required this.user,
    required this.username,
  });

  factory Profile.fromJson(Map<String, dynamic> json) => Profile(
        avatarUrl: json["avatar_url"],
        created: json["created"],
        isEditor: json["is_editor"],
        updated: json["updated"],
        user: json["user"],
        username: json["username"],
      );

  Map<String, dynamic> toJson() => {
        "avatar_url": avatarUrl,
        "created": created,
        "is_editor": isEditor,
        "updated": updated,
        "user": user,
        "username": username,
      };
}
