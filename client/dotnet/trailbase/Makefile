release: clean
	dotnet build -c Release && dotnet pack && dotnet nuget push bin/Release/TrailBase.${VERSION}.nupkg --api-key ${NUGET_KEY} --source https://api.nuget.org/v3/index.json

docs:
	cd ../docs && docfx docfx.json --serve

clean:
	rm -rf bin obj

.PHONY: release clean docs
