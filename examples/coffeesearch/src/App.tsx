import { useState, useEffect } from "react";
import "./App.css";

type Data = Array<Array<object>>;

async function fetchData(v: {
  aroma: number;
  flavor: number;
  acidity: number;
  sweetness: number;
}): Promise<Data> {
  const URL = import.meta.env.DEV ? "http://localhost:4000" : "";
  const params = Object.entries(v)
    .map(([k, v]) => `${k}=${v}`)
    .join("&");
  const response = await fetch(`${URL}/search?${params}`);
  return await response.json();
}

const Input = (props: {
  label: string;
  value: number;
  update: (v: number) => void;
}) => (
  <>
    <label>{props.label}:</label>
    <input
      type="number"
      step="0.1"
      value={props.value}
      onChange={(el) => props.update(el.target.valueAsNumber)}
    />
  </>
);

const Row = (props: { row: Array<object> }) => (
  <tr>
    {props.row.map((d) => (
      <td>{`${typeof d === "number" ? (d as number).toPrecision(3) : d}`}</td>
    ))}
  </tr>
);

function Table() {
  const [aroma, setAroma] = useState(8);
  const [flavor, setFlavor] = useState(8);
  const [acidity, setAcidity] = useState(8);
  const [sweetness, setSweetness] = useState(8);

  const [data, setData] = useState<Data | undefined>();
  useEffect(() => {
    setData(undefined);
    fetchData({ aroma, flavor, acidity, sweetness }).then(setData);
  }, [aroma, flavor, acidity, sweetness]);

  return (
    <>
      <div className="inputs">
        <Input label="Aroma" value={aroma} update={setAroma} />
        <Input label="Flavor" value={flavor} update={setFlavor} />
        <Input label="Acidity" value={acidity} update={setAcidity} />
        <Input label="Sweetness" value={sweetness} update={setSweetness} />
      </div>

      <div className="table">
        <table>
          <thead>
            <tr>
              <th scope="col">Owner</th>
              <th scope="col">Aroma</th>
              <th scope="col">Flavor</th>
              <th scope="col">Acidity</th>
              <th scope="col">Sweetness</th>
            </tr>
          </thead>

          <tbody>
            {(data ?? []).map((row) => (
              <Row row={row} />
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

export const App = () => (
  <>
    <h1>Coffee Search</h1>
    <Table />
  </>
);
