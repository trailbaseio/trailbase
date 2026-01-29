# Join-Order Benchmark

The [join-order benchmark](https://github.com/gregrahn/join-order-benchmark) is a standard SQL
benchmark that stresses the query optimizer, as described in the following paper:

Viktor Leis, et al. "How Good Are Query Optimizers, Really?" PVLDB Volume 9, No.
3, 2015 [pdf](http://www.vldb.org/pvldb/vol9/p204-leis.pdf)

We can use this benchmark to test and optimize our default SQLite setup.

## Data Set

The benchmark usses an [IMDB](https://developer.imdb.com/non-commercial-datasets/) dataset, downloaded in May 2013.
The exported CSV files can be downloaded from https://event.cwi.nl/da/job/imdb.tgz.

Unpack the downloaded `imdb.tgz` in `./data` to obtain 21 CSV files corresponding to a table each, totaling 3.7 GiB.
The biggest table has over 36 million records and the smalles only 4.
