import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:logging/logging.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:trailbase/trailbase.dart';

import 'types/article.dart';
import 'src/login.dart';

Future<void> main() async {
  Logger.root.level = Level.INFO;
  Logger.root.onRecord.listen((record) {
    // ignore: avoid_print
    print(
        '${record.level.name}: ${record.time}  ${record.loggerName}: ${record.message}');
  });

  final prefs = await SharedPreferences.getInstance();

  final tokensJson = prefs.getString(_tokensKey);
  Tokens? tokens;
  try {
    tokens = (tokensJson != null && tokensJson.isNotEmpty)
        ? Tokens.fromJson(jsonDecode(tokensJson))
        : null;
  } catch (err) {
    _logger.warning(err);
  }

  final user = ValueNotifier<User?>(null);
  void onAuthChange(Client client, Tokens? tokens) {
    user.value = client.user();
    prefs.setString(_tokensKey, tokens != null ? jsonEncode(tokens) : '');
  }

  const address = 'http://localhost:4000';
  final client = tokens != null
      ? await Client.withTokens(
          address,
          tokens,
          onAuthChange: onAuthChange,
        )
      : Client(address, onAuthChange: onAuthChange);

  runApp(TrailbaseBlog(client, user));
}

class TrailbaseBlog extends StatelessWidget {
  final ValueNotifier<User?> user;
  final Client client;

  const TrailbaseBlog(this.client, this.user, {super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'TrailBase 🚀',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.teal),
        useMaterial3: true,
      ),
      home: Landing(client: client, user: user),
    );
  }
}

class Landing extends StatefulWidget {
  final ValueNotifier<User?> user;
  final Client client;

  const Landing({
    super.key,
    required this.client,
    required this.user,
  });

  @override
  State<Landing> createState() => _LandingState();
}

class _LandingState extends State<Landing> {
  late final _articlesApi = widget.client.records('articles_view');
  final _articlesCtrl = StreamController<List<Article>>();

  @override
  void initState() {
    super.initState();

    _fetchArticles();
  }

  Future<void> _fetchArticles() async {
    try {
      final response = await _articlesApi.list();
      _articlesCtrl
          .add(response.records.map((r) => Article.fromJson(r)).toList());
    } catch (err) {
      _articlesCtrl.addError(err);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
        title: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            const Text('TrailBase Blog 🚀'),
            ValueListenableBuilder(
              valueListenable: widget.user,
              builder: (BuildContext context, User? user, Widget? _) {
                if (user == null) {
                  return IconButton(
                    icon: const Icon(Icons.no_accounts),
                    onPressed: () => Scaffold.of(context).openEndDrawer(),
                  );
                }

                return Row(
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    Text(user.email),
                    const Icon(Icons.account_box),
                  ],
                );
              },
            ),
          ],
        ),
      ),
      endDrawer: Drawer(
        child: ListView(
          padding: EdgeInsets.zero,
          children: <Widget>[
            const DrawerHeader(
              decoration: BoxDecoration(
                color: Colors.teal,
              ),
              child: Text(
                '',
                style: TextStyle(
                  color: Colors.white,
                  fontSize: 24,
                ),
              ),
            ),
            LoginFormWidget(client: widget.client),
          ],
        ),
      ),
      body: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Expanded(
            child: StreamBuilder(
              stream: _articlesCtrl.stream,
              builder: (
                BuildContext context,
                AsyncSnapshot<List<Article>> articles,
              ) {
                final err = articles.error;
                if (err != null) {
                  return Text(
                      'Stream produced: ${err} ${widget.client.user()}');
                }

                final data = articles.data;
                if (data == null) {
                  return const CircularProgressIndicator();
                }

                return ListView(
                  padding: const EdgeInsets.all(8),
                  children: data
                      .map((a) => ArticleWidget(api: _articlesApi, article: a))
                      .toList(),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class ArticleWidget extends StatelessWidget {
  final RecordApi api;
  final Article article;

  const ArticleWidget({
    super.key,
    required this.api,
    required this.article,
  });

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;

    return Card(
      child: Container(
        padding: const EdgeInsets.all(24),
        child: Row(
          children: [
            if (article.image != null) ...[
              Image.network(
                api.imageUri(RecordId.uuid(article.id), 'image').toString(),
                width: 100,
              ),
              const SizedBox(width: 16),
            ],
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(article.title, style: textTheme.titleLarge),
                  const SizedBox(height: 8),
                  Text(article.intro, style: textTheme.titleMedium),
                  const SizedBox(height: 12),
                  Text(article.body),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

final _logger = Logger('main');
const _tokensKey = 'tokens';
