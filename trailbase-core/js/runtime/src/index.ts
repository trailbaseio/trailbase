declare global {
  function __dispatch(
    m: Method,
    route: string,
    uri: string,
    path: [string, string][],
    headers: [string, string][],
    user: UserType | undefined,
    body: Uint8Array,
  ): Promise<ResponseType>;

  function __dispatchCron(id: number): Promise<string | undefined>;

  var rustyscript: {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    functions: any;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    async_functions: any;
  };
}

declare namespace Deno {
  export interface ReadFileOptions {
    /**
     * An abort signal to allow cancellation of the file read operation.
     * If the signal becomes aborted the readFile operation will be stopped
     * and the promise returned will be rejected with an AbortError.
     */
    signal?: AbortSignal;
  }

  export interface WriteFileOptions {
    /** If set to `true`, will append to a file instead of overwriting previous
     * contents.
     *
     * @default {false} */
    append?: boolean;
    /** Sets the option to allow creating a new file, if one doesn't already
     * exist at the specified path.
     *
     * @default {true} */
    create?: boolean;
    /** If set to `true`, no file, directory, or symlink is allowed to exist at
     * the target location. When createNew is set to `true`, `create` is ignored.
     *
     * @default {false} */
    createNew?: boolean;
    /** Permissions always applied to file. */
    mode?: number;
    /** An abort signal to allow cancellation of the file write operation.
     *
     * If the signal becomes aborted the write file operation will be stopped
     * and the promise returned will be rejected with an {@linkcode AbortError}.
     */
    signal?: AbortSignal;
  }

  /**
   * Options which can be set when using {@linkcode Deno.makeTempDir},
   * {@linkcode Deno.makeTempDirSync}, {@linkcode Deno.makeTempFile}, and
   * {@linkcode Deno.makeTempFileSync}.
   *
   * @category File System */
  export interface MakeTempOptions {
    /** Directory where the temporary directory should be created (defaults to
     * the env variable `TMPDIR`, or the system's default, usually `/tmp`).
     *
     * Note that if the passed `dir` is relative, the path returned by
     * `makeTempFile()` and `makeTempDir()` will also be relative. Be mindful of
     * this when changing working directory. */
    dir?: string;
    /** String that should precede the random portion of the temporary
     * directory's name. */
    prefix?: string;
    /** String that should follow the random portion of the temporary
     * directory's name. */
    suffix?: string;
  }

  /**
   * Options which can be set when using {@linkcode Deno.mkdir} and
   * {@linkcode Deno.mkdirSync}.
   *
   * @category File System */
  export interface MkdirOptions {
    /** If set to `true`, means that any intermediate directories will also be
     * created (as with the shell command `mkdir -p`).
     *
     * Intermediate directories are created with the same permissions.
     *
     * When recursive is set to `true`, succeeds silently (without changing any
     * permissions) if a directory already exists at the path, or if the path
     * is a symlink to an existing directory.
     *
     * @default {false} */
    recursive?: boolean;
    /** Permissions to use when creating the directory (defaults to `0o777`,
     * before the process's umask).
     *
     * Ignored on Windows. */
    mode?: number;
  }

  /**
   * Information about a directory entry returned from {@linkcode Deno.readDir}
   * and {@linkcode Deno.readDirSync}.
   *
   * @category File System */
  export interface DirEntry {
    /** The file name of the entry. It is just the entity name and does not
     * include the full path. */
    name: string;
    /** True if this is info for a regular file. Mutually exclusive to
     * `DirEntry.isDirectory` and `DirEntry.isSymlink`. */
    isFile: boolean;
    /** True if this is info for a regular directory. Mutually exclusive to
     * `DirEntry.isFile` and `DirEntry.isSymlink`. */
    isDirectory: boolean;
    /** True if this is info for a symlink. Mutually exclusive to
     * `DirEntry.isFile` and `DirEntry.isDirectory`. */
    isSymlink: boolean;
  }

  /**
   * Options which can be set when doing {@linkcode Deno.open} and
   * {@linkcode Deno.openSync}.
   *
   * @category File System */
  export interface OpenOptions {
    /** Sets the option for read access. This option, when `true`, means that
     * the file should be read-able if opened.
     *
     * @default {true} */
    read?: boolean;
    /** Sets the option for write access. This option, when `true`, means that
     * the file should be write-able if opened. If the file already exists,
     * any write calls on it will overwrite its contents, by default without
     * truncating it.
     *
     * @default {false} */
    write?: boolean;
    /** Sets the option for the append mode. This option, when `true`, means
     * that writes will append to a file instead of overwriting previous
     * contents.
     *
     * Note that setting `{ write: true, append: true }` has the same effect as
     * setting only `{ append: true }`.
     *
     * @default {false} */
    append?: boolean;
    /** Sets the option for truncating a previous file. If a file is
     * successfully opened with this option set it will truncate the file to `0`
     * size if it already exists. The file must be opened with write access
     * for truncate to work.
     *
     * @default {false} */
    truncate?: boolean;
    /** Sets the option to allow creating a new file, if one doesn't already
     * exist at the specified path. Requires write or append access to be
     * used.
     *
     * @default {false} */
    create?: boolean;
    /** If set to `true`, no file, directory, or symlink is allowed to exist at
     * the target location. Requires write or append access to be used. When
     * createNew is set to `true`, create and truncate are ignored.
     *
     * @default {false} */
    createNew?: boolean;
    /** Permissions to use if creating the file (defaults to `0o666`, before
     * the process's umask).
     *
     * Ignored on Windows. */
    mode?: number;
  }

  /**
   * Options which can be set when using {@linkcode Deno.remove} and
   * {@linkcode Deno.removeSync}.
   *
   * @category File System */
  export interface RemoveOptions {
    /** If set to `true`, path will be removed even if it's a non-empty directory.
     *
     * @default {false} */
    recursive?: boolean;
  }

  /** Options that can be used with {@linkcode symlink} and
   * {@linkcode symlinkSync}.
   *
   * @category File System */
  export interface SymlinkOptions {
    /** Specify the symbolic link type as file, directory or NTFS junction. This
     * option only applies to Windows and is ignored on other operating systems. */
    type: "file" | "dir" | "junction";
  }

  export function writeFile(
    path: string | URL,
    data: Uint8Array | ReadableStream<Uint8Array>,
    options?: WriteFileOptions,
  ): Promise<void>;

  export function writeTextFile(
    path: string | URL,
    data: string | ReadableStream<string>,
    options?: WriteFileOptions,
  ): Promise<void>;

  export function readTextFile(
    path: string | URL,
    options?: ReadFileOptions,
  ): Promise<string>;

  export function readFile(
    path: string | URL,
    options?: ReadFileOptions,
  ): Promise<Uint8Array>;

  export function chmod(path: string | URL, mode: number): Promise<void>;

  export function chown(
    path: string | URL,
    uid: number | null,
    gid: number | null,
  ): Promise<void>;

  export function cwd(): string;

  export function makeTempDir(options?: MakeTempOptions): Promise<string>;

  export function makeTempFile(options?: MakeTempOptions): Promise<string>;

  export function mkdir(
    path: string | URL,
    options?: MkdirOptions,
  ): Promise<void>;

  export function chdir(directory: string | URL): void;

  export function copyFile(
    fromPath: string | URL,
    toPath: string | URL,
  ): Promise<void>;

  export function readDir(path: string | URL): AsyncIterable<DirEntry>;

  export function readLink(path: string | URL): Promise<string>;

  export function realPath(path: string | URL): Promise<string>;

  export function remove(
    path: string | URL,
    options?: RemoveOptions,
  ): Promise<void>;

  export function rename(
    oldpath: string | URL,
    newpath: string | URL,
  ): Promise<void>;

  export function stat(path: string | URL): Promise<FileInfo>;

  export function lstat(path: string | URL): Promise<FileInfo>;

  export function truncate(name: string, len?: number): Promise<void>;

  export function open(
    path: string | URL,
    options?: OpenOptions,
  ): Promise<FsFile>;

  export function create(path: string | URL): Promise<FsFile>;

  export function symlink(
    oldpath: string | URL,
    newpath: string | URL,
    options?: SymlinkOptions,
  ): Promise<void>;

  export function link(oldpath: string, newpath: string): Promise<void>;

  export function utime(
    path: string | URL,
    atime: number | Date,
    mtime: number | Date,
  ): Promise<void>;

  export function umask(mask?: number): number;

  /** Provides information about a file and is returned by
   * {@linkcode Deno.stat}, {@linkcode Deno.lstat}, {@linkcode Deno.statSync},
   * and {@linkcode Deno.lstatSync} or from calling `stat()` and `statSync()`
   * on an {@linkcode Deno.FsFile} instance.
   *
   * @category File System
   */
  export interface FileInfo {
    /** True if this is info for a regular file. Mutually exclusive to
     * `FileInfo.isDirectory` and `FileInfo.isSymlink`. */
    isFile: boolean;
    /** True if this is info for a regular directory. Mutually exclusive to
     * `FileInfo.isFile` and `FileInfo.isSymlink`. */
    isDirectory: boolean;
    /** True if this is info for a symlink. Mutually exclusive to
     * `FileInfo.isFile` and `FileInfo.isDirectory`. */
    isSymlink: boolean;
    /** The size of the file, in bytes. */
    size: number;
    /** The last modification time of the file. This corresponds to the `mtime`
     * field from `stat` on Linux/Mac OS and `ftLastWriteTime` on Windows. This
     * may not be available on all platforms. */
    mtime: Date | null;
    /** The last access time of the file. This corresponds to the `atime`
     * field from `stat` on Unix and `ftLastAccessTime` on Windows. This may not
     * be available on all platforms. */
    atime: Date | null;
    /** The creation time of the file. This corresponds to the `birthtime`
     * field from `stat` on Mac/BSD and `ftCreationTime` on Windows. This may
     * not be available on all platforms. */
    birthtime: Date | null;
    /** The last change time of the file. This corresponds to the `ctime`
     * field from `stat` on Mac/BSD and `ChangeTime` on Windows. This may
     * not be available on all platforms. */
    ctime: Date | null;
    /** ID of the device containing the file. */
    dev: number;
    /** Inode number.
     *
     * _Linux/Mac OS only._ */
    ino: number | null;
    /** The underlying raw `st_mode` bits that contain the standard Unix
     * permissions for this file/directory.
     */
    mode: number | null;
    /** Number of hard links pointing to this file.
     *
     * _Linux/Mac OS only._ */
    nlink: number | null;
    /** User ID of the owner of this file.
     *
     * _Linux/Mac OS only._ */
    uid: number | null;
    /** Group ID of the owner of this file.
     *
     * _Linux/Mac OS only._ */
    gid: number | null;
    /** Device ID of this file.
     *
     * _Linux/Mac OS only._ */
    rdev: number | null;
    /** Blocksize for filesystem I/O.
     *
     * _Linux/Mac OS only._ */
    blksize: number | null;
    /** Number of blocks allocated to the file, in 512-byte units.
     *
     * _Linux/Mac OS only._ */
    blocks: number | null;
    /**  True if this is info for a block device.
     *
     * _Linux/Mac OS only._ */
    isBlockDevice: boolean | null;
    /**  True if this is info for a char device.
     *
     * _Linux/Mac OS only._ */
    isCharDevice: boolean | null;
    /**  True if this is info for a fifo.
     *
     * _Linux/Mac OS only._ */
    isFifo: boolean | null;
    /**  True if this is info for a socket.
     *
     * _Linux/Mac OS only._ */
    isSocket: boolean | null;
  }

  /**
   * A enum which defines the seek mode for IO related APIs that support
   * seeking.
   *
   * @category I/O */
  export enum SeekMode {
    /* Seek from the start of the file/resource. */
    Start = 0,
    /* Seek from the current position within the file/resource. */
    Current = 1,
    /* Seek from the end of the current file/resource. */
    End = 2,
  }

  /** @category I/O */
  export interface SetRawOptions {
    /**
     * The `cbreak` option can be used to indicate that characters that
     * correspond to a signal should still be generated. When disabling raw
     * mode, this option is ignored. This functionality currently only works on
     * Linux and Mac OS.
     */
    cbreak: boolean;
  }

  export class FsFile implements Disposable {
    /** A {@linkcode ReadableStream} instance representing to the byte contents
     * of the file. This makes it easy to interoperate with other web streams
     * based APIs.
     *
     * ```ts
     * using file = await Deno.open("my_file.txt", { read: true });
     * const decoder = new TextDecoder();
     * for await (const chunk of file.readable) {
     *   console.log(decoder.decode(chunk));
     * }
     * ```
     */
    readonly readable: ReadableStream<Uint8Array>;
    /** A {@linkcode WritableStream} instance to write the contents of the
     * file. This makes it easy to interoperate with other web streams based
     * APIs.
     *
     * ```ts
     * const items = ["hello", "world"];
     * using file = await Deno.open("my_file.txt", { write: true });
     * const encoder = new TextEncoder();
     * const writer = file.writable.getWriter();
     * for (const item of items) {
     *   await writer.write(encoder.encode(item));
     * }
     * ```
     */
    readonly writable: WritableStream<Uint8Array>;
    /** Write the contents of the array buffer (`p`) to the file.
     *
     * Resolves to the number of bytes written.
     *
     * **It is not guaranteed that the full buffer will be written in a single
     * call.**
     *
     * ```ts
     * const encoder = new TextEncoder();
     * const data = encoder.encode("Hello world");
     * using file = await Deno.open("/foo/bar.txt", { write: true });
     * const bytesWritten = await file.write(data); // 11
     * ```
     *
     * @category I/O
     */
    write(p: Uint8Array): Promise<number>;
    /** Synchronously write the contents of the array buffer (`p`) to the file.
     *
     * Returns the number of bytes written.
     *
     * **It is not guaranteed that the full buffer will be written in a single
     * call.**
     *
     * ```ts
     * const encoder = new TextEncoder();
     * const data = encoder.encode("Hello world");
     * using file = Deno.openSync("/foo/bar.txt", { write: true });
     * const bytesWritten = file.writeSync(data); // 11
     * ```
     */
    writeSync(p: Uint8Array): number;
    /** Truncates (or extends) the file to reach the specified `len`. If `len`
     * is not specified, then the entire file contents are truncated.
     *
     * ### Truncate the entire file
     *
     * ```ts
     * using file = await Deno.open("my_file.txt", { write: true });
     * await file.truncate();
     * ```
     *
     * ### Truncate part of the file
     *
     * ```ts
     * // if "my_file.txt" contains the text "hello world":
     * using file = await Deno.open("my_file.txt", { write: true });
     * await file.truncate(7);
     * const buf = new Uint8Array(100);
     * await file.read(buf);
     * const text = new TextDecoder().decode(buf); // "hello w"
     * ```
     */
    truncate(len?: number): Promise<void>;
    /** Synchronously truncates (or extends) the file to reach the specified
     * `len`. If `len` is not specified, then the entire file contents are
     * truncated.
     *
     * ### Truncate the entire file
     *
     * ```ts
     * using file = Deno.openSync("my_file.txt", { write: true });
     * file.truncateSync();
     * ```
     *
     * ### Truncate part of the file
     *
     * ```ts
     * // if "my_file.txt" contains the text "hello world":
     * using file = Deno.openSync("my_file.txt", { write: true });
     * file.truncateSync(7);
     * const buf = new Uint8Array(100);
     * file.readSync(buf);
     * const text = new TextDecoder().decode(buf); // "hello w"
     * ```
     */
    truncateSync(len?: number): void;
    /** Read the file into an array buffer (`p`).
     *
     * Resolves to either the number of bytes read during the operation or EOF
     * (`null`) if there was nothing more to read.
     *
     * It is possible for a read to successfully return with `0` bytes. This
     * does not indicate EOF.
     *
     * **It is not guaranteed that the full buffer will be read in a single
     * call.**
     *
     * ```ts
     * // if "/foo/bar.txt" contains the text "hello world":
     * using file = await Deno.open("/foo/bar.txt");
     * const buf = new Uint8Array(100);
     * const numberOfBytesRead = await file.read(buf); // 11 bytes
     * const text = new TextDecoder().decode(buf);  // "hello world"
     * ```
     */
    read(p: Uint8Array): Promise<number | null>;
    /** Synchronously read from the file into an array buffer (`p`).
     *
     * Returns either the number of bytes read during the operation or EOF
     * (`null`) if there was nothing more to read.
     *
     * It is possible for a read to successfully return with `0` bytes. This
     * does not indicate EOF.
     *
     * **It is not guaranteed that the full buffer will be read in a single
     * call.**
     *
     * ```ts
     * // if "/foo/bar.txt" contains the text "hello world":
     * using file = Deno.openSync("/foo/bar.txt");
     * const buf = new Uint8Array(100);
     * const numberOfBytesRead = file.readSync(buf); // 11 bytes
     * const text = new TextDecoder().decode(buf);  // "hello world"
     * ```
     */
    readSync(p: Uint8Array): number | null;
    /** Seek to the given `offset` under mode given by `whence`. The call
     * resolves to the new position within the resource (bytes from the start).
     *
     * ```ts
     * // Given the file contains "Hello world" text, which is 11 bytes long:
     * using file = await Deno.open(
     *   "hello.txt",
     *   { read: true, write: true, truncate: true, create: true },
     * );
     * await file.write(new TextEncoder().encode("Hello world"));
     *
     * // advance cursor 6 bytes
     * const cursorPosition = await file.seek(6, Deno.SeekMode.Start);
     * console.log(cursorPosition);  // 6
     * const buf = new Uint8Array(100);
     * await file.read(buf);
     * console.log(new TextDecoder().decode(buf)); // "world"
     * ```
     *
     * The seek modes work as follows:
     *
     * ```ts
     * // Given the file contains "Hello world" text, which is 11 bytes long:
     * const file = await Deno.open(
     *   "hello.txt",
     *   { read: true, write: true, truncate: true, create: true },
     * );
     * await file.write(new TextEncoder().encode("Hello world"));
     *
     * // Seek 6 bytes from the start of the file
     * console.log(await file.seek(6, Deno.SeekMode.Start)); // "6"
     * // Seek 2 more bytes from the current position
     * console.log(await file.seek(2, Deno.SeekMode.Current)); // "8"
     * // Seek backwards 2 bytes from the end of the file
     * console.log(await file.seek(-2, Deno.SeekMode.End)); // "9" (i.e. 11-2)
     * ```
     */
    seek(offset: number | bigint, whence: SeekMode): Promise<number>;
    /** Synchronously seek to the given `offset` under mode given by `whence`.
     * The new position within the resource (bytes from the start) is returned.
     *
     * ```ts
     * using file = Deno.openSync(
     *   "hello.txt",
     *   { read: true, write: true, truncate: true, create: true },
     * );
     * file.writeSync(new TextEncoder().encode("Hello world"));
     *
     * // advance cursor 6 bytes
     * const cursorPosition = file.seekSync(6, Deno.SeekMode.Start);
     * console.log(cursorPosition);  // 6
     * const buf = new Uint8Array(100);
     * file.readSync(buf);
     * console.log(new TextDecoder().decode(buf)); // "world"
     * ```
     *
     * The seek modes work as follows:
     *
     * ```ts
     * // Given the file contains "Hello world" text, which is 11 bytes long:
     * using file = Deno.openSync(
     *   "hello.txt",
     *   { read: true, write: true, truncate: true, create: true },
     * );
     * file.writeSync(new TextEncoder().encode("Hello world"));
     *
     * // Seek 6 bytes from the start of the file
     * console.log(file.seekSync(6, Deno.SeekMode.Start)); // "6"
     * // Seek 2 more bytes from the current position
     * console.log(file.seekSync(2, Deno.SeekMode.Current)); // "8"
     * // Seek backwards 2 bytes from the end of the file
     * console.log(file.seekSync(-2, Deno.SeekMode.End)); // "9" (i.e. 11-2)
     * ```
     */
    seekSync(offset: number | bigint, whence: SeekMode): number;
    /** Resolves to a {@linkcode Deno.FileInfo} for the file.
     *
     * ```ts
     * import { assert } from "jsr:@std/assert";
     *
     * using file = await Deno.open("hello.txt");
     * const fileInfo = await file.stat();
     * assert(fileInfo.isFile);
     * ```
     */
    stat(): Promise<FileInfo>;
    /** Synchronously returns a {@linkcode Deno.FileInfo} for the file.
     *
     * ```ts
     * import { assert } from "jsr:@std/assert";
     *
     * using file = Deno.openSync("hello.txt")
     * const fileInfo = file.statSync();
     * assert(fileInfo.isFile);
     * ```
     */
    statSync(): FileInfo;
    /**
     * Flushes any pending data and metadata operations of the given file
     * stream to disk.
     *
     * ```ts
     * const file = await Deno.open(
     *   "my_file.txt",
     *   { read: true, write: true, create: true },
     * );
     * await file.write(new TextEncoder().encode("Hello World"));
     * await file.truncate(1);
     * await file.sync();
     * console.log(await Deno.readTextFile("my_file.txt")); // H
     * ```
     *
     * @category I/O
     */
    sync(): Promise<void>;
    /**
     * Synchronously flushes any pending data and metadata operations of the given
     * file stream to disk.
     *
     * ```ts
     * const file = Deno.openSync(
     *   "my_file.txt",
     *   { read: true, write: true, create: true },
     * );
     * file.writeSync(new TextEncoder().encode("Hello World"));
     * file.truncateSync(1);
     * file.syncSync();
     * console.log(Deno.readTextFileSync("my_file.txt")); // H
     * ```
     *
     * @category I/O
     */
    syncSync(): void;
    /**
     * Flushes any pending data operations of the given file stream to disk.
     *  ```ts
     * using file = await Deno.open(
     *   "my_file.txt",
     *   { read: true, write: true, create: true },
     * );
     * await file.write(new TextEncoder().encode("Hello World"));
     * await file.syncData();
     * console.log(await Deno.readTextFile("my_file.txt")); // Hello World
     * ```
     *
     * @category I/O
     */
    syncData(): Promise<void>;
    /**
     * Synchronously flushes any pending data operations of the given file stream
     * to disk.
     *
     *  ```ts
     * using file = Deno.openSync(
     *   "my_file.txt",
     *   { read: true, write: true, create: true },
     * );
     * file.writeSync(new TextEncoder().encode("Hello World"));
     * file.syncDataSync();
     * console.log(Deno.readTextFileSync("my_file.txt")); // Hello World
     * ```
     *
     * @category I/O
     */
    syncDataSync(): void;
    /**
     * Changes the access (`atime`) and modification (`mtime`) times of the
     * file stream resource. Given times are either in seconds (UNIX epoch
     * time) or as `Date` objects.
     *
     * ```ts
     * using file = await Deno.open("file.txt", { create: true, write: true });
     * await file.utime(1556495550, new Date());
     * ```
     *
     * @category File System
     */
    utime(atime: number | Date, mtime: number | Date): Promise<void>;
    /**
     * Synchronously changes the access (`atime`) and modification (`mtime`)
     * times of the file stream resource. Given times are either in seconds
     * (UNIX epoch time) or as `Date` objects.
     *
     * ```ts
     * using file = Deno.openSync("file.txt", { create: true, write: true });
     * file.utime(1556495550, new Date());
     * ```
     *
     * @category File System
     */
    utimeSync(atime: number | Date, mtime: number | Date): void;
    /** **UNSTABLE**: New API, yet to be vetted.
     *
     * Checks if the file resource is a TTY (terminal).
     *
     * ```ts
     * // This example is system and context specific
     * using file = await Deno.open("/dev/tty6");
     * file.isTerminal(); // true
     * ```
     */
    isTerminal(): boolean;
    /** **UNSTABLE**: New API, yet to be vetted.
     *
     * Set TTY to be under raw mode or not. In raw mode, characters are read and
     * returned as is, without being processed. All special processing of
     * characters by the terminal is disabled, including echoing input
     * characters. Reading from a TTY device in raw mode is faster than reading
     * from a TTY device in canonical mode.
     *
     * ```ts
     * using file = await Deno.open("/dev/tty6");
     * file.setRaw(true, { cbreak: true });
     * ```
     */
    setRaw(mode: boolean, options?: SetRawOptions): void;
    /**
     * Acquire an advisory file-system lock for the file.
     *
     * @param [exclusive=false]
     */
    lock(exclusive?: boolean): Promise<void>;
    /**
     * Synchronously acquire an advisory file-system lock synchronously for the file.
     *
     * @param [exclusive=false]
     */
    lockSync(exclusive?: boolean): void;
    /**
     * Release an advisory file-system lock for the file.
     */
    unlock(): Promise<void>;
    /**
     * Synchronously release an advisory file-system lock for the file.
     */
    unlockSync(): void;
    /** Close the file. Closing a file when you are finished with it is
     * important to avoid leaking resources.
     *
     * ```ts
     * using file = await Deno.open("my_file.txt");
     * // do work with "file" object
     * ```
     */
    close(): void;

    [Symbol.dispose](): void;
  }
}

// NOTE: Ideally we'd pull in Deno types from https://github.com/denoland/deno/blob/main/cli/tsc/dts/lib.deno.ns.d.ts but haven't found a good way.
export namespace fs {
  export const writeFile = Deno.writeFile;
  export const writeTextFile = Deno.writeTextFile;
  export const readTextFile = Deno.readTextFile;
  export const readFile = Deno.readFile;
  export const chmod = Deno.chmod;
  export const chown = Deno.chown;
  export const cwd = Deno.cwd;
  export const makeTempDir = Deno.makeTempDir;
  export const makeTempFile = Deno.makeTempFile;
  export const mkdir = Deno.mkdir;
  export const chdir = Deno.chdir;
  export const copyFile = Deno.copyFile;
  export const readDir = Deno.readDir;
  export const readLink = Deno.readLink;
  export const realPath = Deno.realPath;
  export const remove = Deno.remove;
  export const rename = Deno.rename;
  export const stat = Deno.stat;
  export const lstat = Deno.lstat;
  export const truncate = Deno.truncate;
  export const FsFile = Deno.FsFile;
  export const open = Deno.open;
  export const create = Deno.create;
  export const symlink = Deno.symlink;
  export const link = Deno.link;
  export const utime = Deno.utime;
  export const umask = Deno.umask;
}

export type HeaderMapType = { [key: string]: string };
export type PathParamsType = { [key: string]: string };
export type UserType = {
  /// Base64 encoded UUIDv7 user id.
  id: string;
  /// The user's email address.
  email: string;
  /// The user's CSRF token.
  csrf: string;
};
export type RequestType = {
  uri: string;
  params: PathParamsType;
  headers: HeaderMapType;
  user?: UserType;
  body?: Uint8Array;
};
export type ResponseType = {
  headers?: [string, string][];
  status?: number;
  body?: Uint8Array;
};
export type MaybeResponse<T> = Promise<T | undefined> | T | undefined;
export type CallbackType = (req: RequestType) => MaybeResponse<ResponseType>;
export type Method =
  | "DELETE"
  | "GET"
  | "HEAD"
  | "OPTIONS"
  | "PATCH"
  | "POST"
  | "PUT"
  | "TRACE";

/// HTTP status codes.
///
// source: https://github.com/prettymuchbryce/http-status-codes/blob/master/src/status-codes.ts
export enum StatusCodes {
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.2.1
  ///
  /// This interim response indicates that everything so far is OK and that the
  /// client should continue with the request or ignore it if it is already
  /// finished.
  CONTINUE = 100,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.2.2
  ///
  /// This code is sent in response to an Upgrade request header by the client,
  /// and indicates the protocol the server is switching too.
  SWITCHING_PROTOCOLS = 101,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.1
  ///
  /// This code indicates that the server has received and is processing the
  /// request, but no response is available yet.
  PROCESSING = 102,
  /// Official Documentation @ https://www.rfc-editor.org/rfc/rfc8297#page-3
  ///
  /// This code indicates to the client that the server is likely to send a
  /// final response with the header fields included in the informational
  /// response.
  EARLY_HINTS = 103,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.1
  ///
  /// The request has succeeded. The meaning of a success varies depending on the HTTP method:
  /// GET: The resource has been fetched and is transmitted in the message body.
  /// HEAD: The entity headers are in the message body.
  /// POST: The resource describing the result of the action is transmitted in the message body.
  /// TRACE: The message body contains the request message as received by the server
  OK = 200,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.2
  ///
  /// The request has succeeded and a new resource has been created as a result
  /// of it. This is typically the response sent after a PUT request.
  CREATED = 201,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.3
  ///
  /// The request has been received but not yet acted upon. It is
  /// non-committal, meaning that there is no way in HTTP to later send an
  /// asynchronous response indicating the outcome of processing the request. It
  /// is intended for cases where another process or server handles the request,
  /// or for batch processing.
  ACCEPTED = 202,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.4
  ///
  /// This response code means returned meta-information set is not exact set
  /// as available from the origin server, but collected from a local or a third
  /// party copy. Except this condition, 200 OK response should be preferred
  /// instead of this response.
  NON_AUTHORITATIVE_INFORMATION = 203,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.5
  ///
  /// There is no content to send for this request, but the headers may be
  /// useful. The user-agent may update its cached headers for this resource with
  /// the new ones.
  NO_CONTENT = 204,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.6
  ///
  /// This response code is sent after accomplishing request to tell user agent
  /// reset document view which sent this request.
  RESET_CONTENT = 205,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7233#section-4.1
  ///
  /// This response code is used because of range header sent by the client to
  /// separate download into multiple streams.
  PARTIAL_CONTENT = 206,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.2
  ///
  /// A Multi-Status response conveys information about multiple resources in
  /// situations where multiple status codes might be appropriate.
  MULTI_STATUS = 207,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.1
  ///
  /// The request has more than one possible responses. User-agent or user
  /// should choose one of them. There is no standardized way to choose one of
  /// the responses.
  MULTIPLE_CHOICES = 300,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.2
  ///
  /// This response code means that URI of requested resource has been changed.
  /// Probably, new URI would be given in the response.
  MOVED_PERMANENTLY = 301,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.3
  ///
  /// This response code means that URI of requested resource has been changed
  /// temporarily. New changes in the URI might be made in the future. Therefore,
  /// this same URI should be used by the client in future requests.
  MOVED_TEMPORARILY = 302,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.4
  ///
  /// Server sent this response to directing client to get requested resource
  /// to another URI with an GET request.
  SEE_OTHER = 303,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7232#section-4.1
  ///
  /// This is used for caching purposes. It is telling to client that response
  /// has not been modified. So, client can continue to use same cached version
  /// of response.
  NOT_MODIFIED = 304,
  /// @deprecated
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.6
  ///
  /// Was defined in a previous version of the HTTP specification to indicate
  /// that a requested response must be accessed by a proxy. It has been
  /// deprecated due to security concerns regarding in-band configuration of a
  /// proxy.
  USE_PROXY = 305,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.7
  ///
  /// Server sent this response to directing client to get requested resource
  /// to another URI with same method that used prior request. This has the same
  /// semantic than the 302 Found HTTP response code, with the exception that the
  /// user agent must not change the HTTP method used: if a POST was used in the
  /// first request, a POST must be used in the second request.
  TEMPORARY_REDIRECT = 307,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7538#section-3
  ///
  /// This means that the resource is now permanently located at another URI,
  /// specified by the Location: HTTP Response header. This has the same
  /// semantics as the 301 Moved Permanently HTTP response code, with the
  /// exception that the user agent must not change the HTTP method used: if a
  /// POST was used in the first request, a POST must be used in the second
  /// request.
  PERMANENT_REDIRECT = 308,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.1
  ///
  /// This response means that server could not understand the request due to invalid syntax.
  BAD_REQUEST = 400,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7235#section-3.1
  ///
  /// Although the HTTP standard specifies "unauthorized", semantically this
  /// response means "unauthenticated". That is, the client must authenticate
  /// itself to get the requested response.
  UNAUTHORIZED = 401,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.2
  ///
  /// This response code is reserved for future use. Initial aim for creating
  /// this code was using it for digital payment systems however this is not used
  /// currently.
  PAYMENT_REQUIRED = 402,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.3
  ///
  /// The client does not have access rights to the content, i.e. they are
  /// unauthorized, so server is rejecting to give proper response. Unlike 401,
  /// the client's identity is known to the server.
  FORBIDDEN = 403,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.4
  ///
  /// The server can not find requested resource. In the browser, this means
  /// the URL is not recognized. In an API, this can also mean that the endpoint
  /// is valid but the resource itself does not exist. Servers may also send this
  /// response instead of 403 to hide the existence of a resource from an
  /// unauthorized client. This response code is probably the most famous one due
  /// to its frequent occurence on the web.
  NOT_FOUND = 404,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.5
  ///
  /// The request method is known by the server but has been disabled and
  /// cannot be used. For example, an API may forbid DELETE-ing a resource. The
  /// two mandatory methods, GET and HEAD, must never be disabled and should not
  /// return this error code.
  METHOD_NOT_ALLOWED = 405,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.6
  ///
  /// This response is sent when the web server, after performing server-driven
  /// content negotiation, doesn't find any content following the criteria given
  /// by the user agent.
  NOT_ACCEPTABLE = 406,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7235#section-3.2
  ///
  /// This is similar to 401 but authentication is needed to be done by a proxy.
  PROXY_AUTHENTICATION_REQUIRED = 407,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.7
  ///
  /// This response is sent on an idle connection by some servers, even without
  /// any previous request by the client. It means that the server would like to
  /// shut down this unused connection. This response is used much more since
  /// some browsers, like Chrome, Firefox 27+, or IE9, use HTTP pre-connection
  /// mechanisms to speed up surfing. Also note that some servers merely shut
  /// down the connection without sending this message.
  REQUEST_TIMEOUT = 408,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.8
  ///
  /// This response is sent when a request conflicts with the current state of the server.
  CONFLICT = 409,
  ///
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.9
  ///
  /// This response would be sent when the requested content has been
  /// permenantly deleted from server, with no forwarding address. Clients are
  /// expected to remove their caches and links to the resource. The HTTP
  /// specification intends this status code to be used for "limited-time,
  /// promotional services". APIs should not feel compelled to indicate resources
  /// that have been deleted with this status code.
  GONE = 410,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.10
  ///
  /// The server rejected the request because the Content-Length header field
  /// is not defined and the server requires it.
  LENGTH_REQUIRED = 411,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7232#section-4.2
  ///
  /// The client has indicated preconditions in its headers which the server
  /// does not meet.
  PRECONDITION_FAILED = 412,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.11
  ///
  /// Request entity is larger than limits defined by server; the server might
  /// close the connection or return an Retry-After header field.
  REQUEST_TOO_LONG = 413,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.12
  ///
  /// The URI requested by the client is longer than the server is willing to interpret.
  REQUEST_URI_TOO_LONG = 414,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.13
  ///
  /// The media format of the requested data is not supported by the server, so
  /// the server is rejecting the request.
  UNSUPPORTED_MEDIA_TYPE = 415,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7233#section-4.4
  ///
  /// The range specified by the Range header field in the request can't be
  /// fulfilled; it's possible that the range is outside the size of the target
  /// URI's data.
  REQUESTED_RANGE_NOT_SATISFIABLE = 416,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.14
  ///
  /// This response code means the expectation indicated by the Expect request
  /// header field can't be met by the server.
  EXPECTATION_FAILED = 417,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2324#section-2.3.2
  ///
  /// Any attempt to brew coffee with a teapot should result in the error code
  /// "418 I'm a teapot". The resulting entity body MAY be short and stout.
  IM_A_TEAPOT = 418,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.6
  ///
  /// The 507 (Insufficient Storage) status code means the method could not be
  /// performed on the resource because the server is unable to store the
  /// representation needed to successfully complete the request. This condition
  /// is considered to be temporary. If the request which received this status
  /// code was the result of a user action, the request MUST NOT be repeated
  /// until it is requested by a separate user action.
  INSUFFICIENT_SPACE_ON_RESOURCE = 419,
  /// @deprecated
  /// Official Documentation @ https://tools.ietf.org/rfcdiff?difftype=--hwdiff&url2=draft-ietf-webdav-protocol-06.txt
  ///
  /// A deprecated response used by the Spring Framework when a method has failed.
  METHOD_FAILURE = 420,
  /// Official Documentation @ https://datatracker.ietf.org/doc/html/rfc7540#section-9.1.2
  ///
  /// Defined in the specification of HTTP/2 to indicate that a server is not
  /// able to produce a response for the combination of scheme and authority that
  /// are included in the request URI.
  MISDIRECTED_REQUEST = 421,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.3
  ///
  /// The request was well-formed but was unable to be followed due to semantic errors.
  UNPROCESSABLE_ENTITY = 422,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.4
  ///
  /// The resource that is being accessed is locked.
  LOCKED = 423,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.5
  ///
  /// The request failed due to failure of a previous request.
  FAILED_DEPENDENCY = 424,
  /// Official Documentation @ https://datatracker.ietf.org/doc/html/rfc7231#section-6.5.15
  ///
  /// The server refuses to perform the request using the current protocol but
  /// might be willing to do so after the client upgrades to a different
  /// protocol.
  UPGRADE_REQUIRED = 426,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-3
  ///
  /// The origin server requires the request to be conditional. Intended to
  /// prevent the 'lost update' problem, where a client GETs a resource's state,
  /// modifies it, and PUTs it back to the server, when meanwhile a third party
  /// has modified the state on the server, leading to a conflict.
  PRECONDITION_REQUIRED = 428,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-4
  ///
  /// The user has sent too many requests in a given amount of time ("rate limiting").
  TOO_MANY_REQUESTS = 429,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-5
  ///
  /// The server is unwilling to process the request because its header fields
  /// are too large. The request MAY be resubmitted after reducing the size of
  /// the request header fields.
  REQUEST_HEADER_FIELDS_TOO_LARGE = 431,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7725
  ///
  /// The user-agent requested a resource that cannot legally be provided, such
  /// as a web page censored by a government.
  UNAVAILABLE_FOR_LEGAL_REASONS = 451,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.1
  ///
  /// The server encountered an unexpected condition that prevented it from
  /// fulfilling the request.
  INTERNAL_SERVER_ERROR = 500,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.2
  ///
  /// The request method is not supported by the server and cannot be handled.
  /// The only methods that servers are required to support (and therefore that
  /// must not return this code) are GET and HEAD.
  NOT_IMPLEMENTED = 501,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.3
  ///
  /// This error response means that the server, while working as a gateway to
  /// get a response needed to handle the request, got an invalid response.
  BAD_GATEWAY = 502,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.4
  ///
  /// The server is not ready to handle the request. Common causes are a server
  /// that is down for maintenance or that is overloaded. Note that together with
  /// this response, a user-friendly page explaining the problem should be sent.
  /// This responses should be used for temporary conditions and the Retry-After:
  /// HTTP header should, if possible, contain the estimated time before the
  /// recovery of the service. The webmaster must also take care about the
  /// caching-related headers that are sent along with this response, as these
  /// temporary condition responses should usually not be cached.
  SERVICE_UNAVAILABLE = 503,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.5
  ///
  /// This error response is given when the server is acting as a gateway and
  /// cannot get a response in time.
  GATEWAY_TIMEOUT = 504,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.6
  ///
  /// The HTTP version used in the request is not supported by the server.
  HTTP_VERSION_NOT_SUPPORTED = 505,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.6
  ///
  /// The server has an internal configuration error: the chosen variant
  /// resource is configured to engage in transparent content negotiation itself,
  /// and is therefore not a proper end point in the negotiation process.
  INSUFFICIENT_STORAGE = 507,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-6
  ///
  /// The 511 status code indicates that the client needs to authenticate to
  /// gain network access.
  NETWORK_AUTHENTICATION_REQUIRED = 511,
}

export class HttpError extends Error {
  readonly statusCode: number;
  readonly headers: [string, string][] | undefined;

  constructor(
    statusCode: number,
    message?: string,
    headers?: [string, string][],
  ) {
    super(message);
    this.statusCode = statusCode;
    this.headers = headers;
  }

  public override toString(): string {
    return `HttpError(${this.statusCode}, ${this.message})`;
  }

  toResponse(): ResponseType {
    const m = this.message;
    return {
      headers: this.headers,
      status: this.statusCode,
      body: m !== "" ? encodeFallback(m) : undefined,
    };
  }
}

export type StringRequestType = {
  uri: string;
  params: PathParamsType;
  headers: HeaderMapType;
  user?: UserType;
  body?: string;
};
export type StringResponseType = {
  headers?: [string, string][];
  status?: number;
  body: string;
};

export function stringHandler(
  f: (req: StringRequestType) => MaybeResponse<StringResponseType | string>,
): CallbackType {
  return async (req: RequestType): Promise<ResponseType | undefined> => {
    try {
      const body = req.body;
      const resp: StringResponseType | string | undefined = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body),
      });

      if (resp === undefined) {
        return undefined;
      }

      if (typeof resp === "string") {
        return {
          status: StatusCodes.OK,
          body: encodeFallback(resp),
        };
      }

      const respBody = resp.body;
      return {
        headers: resp.headers,
        status: resp.status,
        body: respBody ? encodeFallback(respBody) : undefined,
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        status: StatusCodes.INTERNAL_SERVER_ERROR,
        body: encodeFallback(`Uncaught error: ${err}`),
      };
    }
  };
}

export type HtmlResponseType = {
  headers?: [string, string][];
  status?: number;
  body: string;
};

export function htmlHandler(
  f: (req: StringRequestType) => MaybeResponse<HtmlResponseType | string>,
): CallbackType {
  return async (req: RequestType): Promise<ResponseType | undefined> => {
    try {
      const body = req.body;
      const resp: HtmlResponseType | string | undefined = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body),
      });

      if (resp === undefined) {
        return undefined;
      }

      if (typeof resp === "string") {
        return {
          headers: [["content-type", "text/html"]],
          status: StatusCodes.OK,
          body: encodeFallback(resp),
        };
      }

      const respBody = resp.body;
      return {
        headers: [["content-type", "text/html"], ...(resp.headers ?? [])],
        status: resp.status,
        body: respBody ? encodeFallback(respBody) : undefined,
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        status: StatusCodes.INTERNAL_SERVER_ERROR,
        body: encodeFallback(`Uncaught error: ${err}`),
      };
    }
  };
}

export type JsonRequestType = {
  uri: string;
  params: PathParamsType;
  headers: HeaderMapType;
  user?: UserType;
  body?: object | string;
};
export interface JsonResponseType {
  headers?: [string, string][];
  status?: number;
  body: object;
}

export function jsonHandler(
  f: (req: JsonRequestType) => MaybeResponse<JsonRequestType | object>,
): CallbackType {
  return async (req: RequestType): Promise<ResponseType | undefined> => {
    try {
      const body = req.body;
      const resp: JsonResponseType | object | undefined = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body),
      });

      if (resp === undefined) {
        return undefined;
      }

      if ("body" in resp) {
        const r = resp as JsonResponseType;
        const rBody = r.body;
        return {
          headers: [["content-type", "application/json"], ...(r.headers ?? [])],
          status: r.status,
          body: rBody ? encodeFallback(JSON.stringify(rBody)) : undefined,
        };
      }

      return {
        headers: [["content-type", "application/json"]],
        status: StatusCodes.OK,
        body: encodeFallback(JSON.stringify(resp)),
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        headers: [["content-type", "application/json"]],
        status: StatusCodes.INTERNAL_SERVER_ERROR,
        body: encodeFallback(`Uncaught error: ${err}`),
      };
    }
  };
}

const routerCallbacks = new Map<string, CallbackType>();

function isolateId(): number {
  return rustyscript.functions.isolate_id();
}

export function addRoute(
  method: Method,
  route: string,
  callback: CallbackType,
) {
  if (isolateId() === 0) {
    rustyscript.functions.install_route(method, route);
    console.debug("JS: Added route:", method, route);
  }

  routerCallbacks.set(`${method}:${route}`, callback);
}

async function dispatch(
  method: Method,
  route: string,
  uri: string,
  pathParams: [string, string][],
  headers: [string, string][],
  user: UserType | undefined,
  body: Uint8Array,
): Promise<ResponseType> {
  const key = `${method}:${route}`;
  const cb: CallbackType | undefined = routerCallbacks.get(key);
  if (!cb) {
    throw Error(`Missing callback: ${key}`);
  }

  return (
    (await cb({
      uri,
      params: Object.fromEntries(pathParams),
      headers: Object.fromEntries(headers),
      user: user,
      body,
    })) ?? { status: StatusCodes.OK }
  );
}

globalThis.__dispatch = dispatch;

let cronId = 1000;
const cronCallbacks = new Map<number, () => void | Promise<void>>();

/// Installs a Cron job that is registered to be orchestrated from native code.
export function addCronCallback(
  name: string,
  schedule: string,
  cb: () => void | Promise<void>,
) {
  const cronRegex =
    /^(@(yearly|monthly|weekly|daily|hourly|))|((((\d+,)+\d+|(\d+(\/|-)\d+)|\d+|\*)\s*){6,7})$/;

  const matches = cronRegex.test(schedule);
  if (!matches) {
    throw Error(`Not a valid 6/7-component cron schedule: ${schedule}`);
  }

  const id = cronId++;

  if (isolateId() === 0) {
    rustyscript.functions.install_job(id, name, schedule);
    console.debug("JS: add cron callback", id, name);
  }

  cronCallbacks.set(id, cb);
}

async function dispatchCron(id: number): Promise<string | undefined> {
  const cb: (() => void | Promise<void>) | undefined = cronCallbacks.get(id);
  if (!cb) {
    throw Error(`Missing cron callback: ${id}`);
  }

  try {
    await cb();
  } catch (err) {
    return `${err}`;
  }
}

globalThis.__dispatchCron = dispatchCron;

/// Installs a periodic callback in a single isolate and returns a cleanup function.
export function addPeriodicCallback(
  milliseconds: number,
  cb: (cancel: () => void) => void,
): () => void {
  // Note: right now we run periodic tasks only on the first isolate. This is
  // very simple but doesn't use other workers. This has nice properties in
  // terms of state management and hopefully work-stealing will alleviate the
  // issue, i.e. workers will pick up the slack in terms of incoming requests.
  if (isolateId() !== 0) {
    return () => {};
  }

  const handle: number = setInterval(() => {
    cb(() => clearInterval(handle));
  }, milliseconds);

  return () => clearInterval(handle);
}

/// Queries the SQLite database.
export async function query(
  queryStr: string,
  params: unknown[],
): Promise<unknown[][]> {
  return await rustyscript.async_functions.query(queryStr, params);
}

/// Executes given query against SQLite database.
export async function execute(
  queryStr: string,
  params: unknown[],
): Promise<number> {
  return await rustyscript.async_functions.execute(queryStr, params);
}

export type ParsedPath = {
  path: string;
  query: URLSearchParams;
};

export function parsePath(path: string): ParsedPath {
  const queryIndex = path.indexOf("?");
  if (queryIndex >= 0) {
    return {
      path: path.slice(0, queryIndex),
      query: new URLSearchParams(path.slice(queryIndex + 1)),
    };
  }

  return {
    path,
    query: new URLSearchParams(),
  };
}

/// @param {Uint8Array} bytes
/// @return {string}
///
/// source: https://github.com/samthor/fast-text-encoding
function decodeFallback(bytes: Uint8Array): string {
  var inputIndex = 0;

  // Create a working buffer for UTF-16 code points, but don't generate one
  // which is too large for small input sizes. UTF-8 to UCS-16 conversion is
  // going to be at most 1:1, if all code points are ASCII. The other extreme
  // is 4-byte UTF-8, which results in two UCS-16 points, but this is still 50%
  // fewer entries in the output.
  var pendingSize = Math.min(256 * 256, bytes.length + 1);
  var pending = new Uint16Array(pendingSize);
  var chunks = [];
  var pendingIndex = 0;

  for (;;) {
    var more = inputIndex < bytes.length;

    // If there's no more data or there'd be no room for two UTF-16 values,
    // create a chunk. This isn't done at the end by simply slicing the data
    // into equal sized chunks as we might hit a surrogate pair.
    if (!more || pendingIndex >= pendingSize - 1) {
      // nb. .apply and friends are *really slow*. Low-hanging fruit is to
      // expand this to literally pass pending[0], pending[1], ... etc, but
      // the output code expands pretty fast in this case.
      // These extra vars get compiled out: they're just to make TS happy.
      // Turns out you can pass an ArrayLike to .apply().
      var subarray = pending.subarray(0, pendingIndex);
      var arraylike = subarray as unknown as number[];
      chunks.push(String.fromCharCode.apply(null, arraylike));

      if (!more) {
        return chunks.join("");
      }

      // Move the buffer forward and create another chunk.
      bytes = bytes.subarray(inputIndex);
      inputIndex = 0;
      pendingIndex = 0;
    }

    // The native TextDecoder will generate "REPLACEMENT CHARACTER" where the
    // input data is invalid. Here, we blindly parse the data even if it's
    // wrong: e.g., if a 3-byte sequence doesn't have two valid continuations.

    var byte1 = bytes[inputIndex++];
    if ((byte1 & 0x80) === 0) {
      // 1-byte or null
      pending[pendingIndex++] = byte1;
    } else if ((byte1 & 0xe0) === 0xc0) {
      // 2-byte
      var byte2 = bytes[inputIndex++] & 0x3f;
      pending[pendingIndex++] = ((byte1 & 0x1f) << 6) | byte2;
    } else if ((byte1 & 0xf0) === 0xe0) {
      // 3-byte
      var byte2 = bytes[inputIndex++] & 0x3f;
      var byte3 = bytes[inputIndex++] & 0x3f;
      pending[pendingIndex++] = ((byte1 & 0x1f) << 12) | (byte2 << 6) | byte3;
    } else if ((byte1 & 0xf8) === 0xf0) {
      // 4-byte
      var byte2 = bytes[inputIndex++] & 0x3f;
      var byte3 = bytes[inputIndex++] & 0x3f;
      var byte4 = bytes[inputIndex++] & 0x3f;

      // this can be > 0xffff, so possibly generate surrogates
      var codepoint =
        ((byte1 & 0x07) << 0x12) | (byte2 << 0x0c) | (byte3 << 0x06) | byte4;
      if (codepoint > 0xffff) {
        // codepoint &= ~0x10000;
        codepoint -= 0x10000;
        pending[pendingIndex++] = ((codepoint >>> 10) & 0x3ff) | 0xd800;
        codepoint = 0xdc00 | (codepoint & 0x3ff);
      }
      pending[pendingIndex++] = codepoint;
    } else {
      // invalid initial byte
    }
  }
}

/// @param {string} string
/// @return {Uint8Array}
////
/// source: https://github.com/samthor/fast-text-encoding
function encodeFallback(string: string): Uint8Array {
  var pos = 0;
  var len = string.length;

  var at = 0; // output position
  var tlen = Math.max(32, len + (len >>> 1) + 7); // 1.5x size
  var target = new Uint8Array((tlen >>> 3) << 3); // ... but at 8 byte offset

  while (pos < len) {
    var value = string.charCodeAt(pos++);
    if (value >= 0xd800 && value <= 0xdbff) {
      // high surrogate
      if (pos < len) {
        var extra = string.charCodeAt(pos);
        if ((extra & 0xfc00) === 0xdc00) {
          ++pos;
          value = ((value & 0x3ff) << 10) + (extra & 0x3ff) + 0x10000;
        }
      }
      if (value >= 0xd800 && value <= 0xdbff) {
        continue; // drop lone surrogate
      }
    }

    // expand the buffer if we couldn't write 4 bytes
    if (at + 4 > target.length) {
      tlen += 8; // minimum extra
      tlen *= 1.0 + (pos / string.length) * 2; // take 2x the remaining
      tlen = (tlen >>> 3) << 3; // 8 byte offset

      var update = new Uint8Array(tlen);
      update.set(target);
      target = update;
    }

    if ((value & 0xffffff80) === 0) {
      // 1-byte
      target[at++] = value; // ASCII
      continue;
    } else if ((value & 0xfffff800) === 0) {
      // 2-byte
      target[at++] = ((value >>> 6) & 0x1f) | 0xc0;
    } else if ((value & 0xffff0000) === 0) {
      // 3-byte
      target[at++] = ((value >>> 12) & 0x0f) | 0xe0;
      target[at++] = ((value >>> 6) & 0x3f) | 0x80;
    } else if ((value & 0xffe00000) === 0) {
      // 4-byte
      target[at++] = ((value >>> 18) & 0x07) | 0xf0;
      target[at++] = ((value >>> 12) & 0x3f) | 0x80;
      target[at++] = ((value >>> 6) & 0x3f) | 0x80;
    } else {
      continue; // out of range
    }

    target[at++] = (value & 0x3f) | 0x80;
  }

  // Use subarray if slice isn't supported (IE11). This will use more memory
  // because the original array still exists.
  return target.slice ? target.slice(0, at) : target.subarray(0, at);
}
