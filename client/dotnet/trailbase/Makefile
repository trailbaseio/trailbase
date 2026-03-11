test:
	dotnet test ../test

# Publish to http://nuget.org
release: clean
	dotnet build -c Release \
		&& dotnet pack \
	        && dotnet nuget push bin/Release/TrailBase.${VERSION}.nupkg --api-key ${NUGET_KEY} --source https://api.nuget.org/v3/index.json

format:
	dotnet format . ; dotnet format ../test

docs:
	cd ../docs && docfx docfx.json --serve

clean:
	rm -rf bin obj

.PHONY: release clean docs format test
