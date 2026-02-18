# LiteGIS

A GIS extension for sqlite, similar to spatialite and PostGIS.

## References

* PostGIS: https://postgis.net/docs/reference.html
* Spatialite: https://gaia-gis.it/gaia-sins/spatialite-sql-5.1.0.html


## Notes

Note that the pure rust crates like `geo` and `wkb` do not (yet) support SRIDs, we
thus use the C-wrapper `geos`.
