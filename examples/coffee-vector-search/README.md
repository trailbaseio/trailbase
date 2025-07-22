# Coffee Vector Search

A small web application demonstrating the use of TrailBase and its vector
search to build a coffee search.

To import the coffee data from CSV, run:

```bash
$ mkdir -p traildepot/data
$ cat import.sql | sqlite3 traildepot/data/main.db -
```

To build the web app

```bash
$ pnpm i
$ pnpm build
```

Finally, to build the docker container:

```
$ docker build . -t coffee && docker run -p 4001:4000 coffee
```

## Reference

- Coffee data [source](https://github.com/jldbc/coffee-quality-database/blob/master/data/arabica_data_cleaned.csv)
