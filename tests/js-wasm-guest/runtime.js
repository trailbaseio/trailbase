import { getRandomBytes as _ } from 'wasi:random/random@0.2.3';
import { getDirectories } from 'wasi:filesystem/preopens@0.2.3';

export function listDirectories() {
  return getDirectories().map(([_fd, name]) => {
    return name;
  });
}
