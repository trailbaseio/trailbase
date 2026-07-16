class User {
  final String id;
  final String? email;
  final String? username;

  const User({
    required this.id,
    required this.email,
    required this.username,
  });

  @override
  String toString() => 'User(id=${id}, email=${email}, username=${username})';

  @override
  bool operator ==(Object other) {
    return other is User &&
        id == other.id &&
        email == other.email &&
        username == other.username;
  }

  @override
  int get hashCode => Object.hash(id, email, username);
}
