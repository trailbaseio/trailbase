import { useState, useEffect } from "react";
import "./App.css";

const Input = (props: {
  label: string;
  value: number;
  update: (v: number) => void;
}) => (
  <>
    <label>{props.label}:</label>
    <input
      type="number"
      step={0.1}
      max={10}
      min={0}
      value={props.value}
      onChange={(e) => props.update(e.target.valueAsNumber)}
    />
  </>
);

function Table() {
  const [aroma, setAroma] = useState(8);
  const [flavor, setFlavor] = useState(8);
  const [acidity, setAcidity] = useState(8);
  const [sweetness, setSweetness] = useState(8);

  type Record = [string, number, number, number, number];
  const [data, setData] = useState<Record[]>([]);

  useEffect(() => {
    const URL = import.meta.env.DEV ? "http://localhost:4000" : "";
    const params = new URLSearchParams({
      aroma: `${aroma}`,
      flavor: `${flavor}`,
      acidity: `${acidity}`,
      sweetness: `${sweetness}`,
    });

    fetch(`${URL}/search?${params}`).then(async (r) => setData(await r.json()));
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
              <th>Owner</th>
              <th>Aroma</th>
              <th>Flavor</th>
              <th>Acidity</th>
              <th>Sweetness</th>
            </tr>
          </thead>

          <tbody>
            {data.map((row) => (
              <tr>
                {row.map((d) => (
                  <td>{d.toString()}</td>
                ))}
              </tr>
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
