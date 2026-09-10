import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

export function BlobEncodingSelector(props: {
  encoding: BlobEncoding;
  setEncoding: (v: BlobEncoding) => void;
}) {
  return (
    <div class="flex items-center gap-2">
      <Label>Blobs:</Label>

      <Select
        multiple={false}
        options={[...blobEncodings]}
        value={props.encoding}
        itemComponent={(props) => (
          <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
        )}
        onChange={(encoding: BlobEncoding | null) => {
          if (encoding !== null) {
            props.setEncoding(encoding);
          }
        }}
      >
        <SelectTrigger>
          <SelectValue<string>>{(state) => state.selectedOption()}</SelectValue>
        </SelectTrigger>

        <SelectContent />
      </Select>
    </div>
  );
}

const blobEncodings = ["base64", "hex", "mixed"] as const;
export type BlobEncoding = (typeof blobEncodings)[number];
