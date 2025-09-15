bin/Release/net10.0/wasi-wasm/native/Guest.wasm: Library.cs Guest.csproj
	dotnet build -c Release

deploy: bin/Release/net10.0/wasi-wasm/native/Guest.wasm
	cp $< ../../client/testfixture/wasm/

clean:
	rm -rf bin obj

.PHONY: clean deploy
