import { useState } from "react";
import {
  ResponsiveContainer, BarChart, Bar, XAxis, YAxis, Tooltip, CartesianGrid, Legend,
} from "recharts";
import type { AppState, DailyEntry } from "../types";
import { fmtTokens, fmtCost } from "../types";

interface Props { state: AppState }

type Range = 7 | 30 | 90;

function shortDate(period: string): string {
  const [, m, d] = period.split("-");
  return `${parseInt(m)}/${parseInt(d)}`;
}

export default function History({ state }: Props) {
  const [range, setRange] = useState<Range>(30);

  const history: DailyEntry[] = (state.ccusage?.history ?? [])
    .slice()
    .sort((a, b) => a.period.localeCompare(b.period))
    .slice(-range);

  const chartData = history.map(d => ({
    date:   shortDate(d.period),
    period: d.period,
    input:  Math.round(d.input_tokens  / 1000),
    output: Math.round(d.output_tokens / 1000),
    cache:  Math.round((d.cache_read_tokens + d.cache_write_tokens) / 1000),
    total:  Math.round(d.total_tokens / 1000),
    cost:   d.cost_usd,
  }));

  const totalTokens = history.reduce((s, d) => s + d.total_tokens, 0);
  const totalCost   = history.reduce((s, d) => s + d.cost_usd,     0);

  return (
    <div className="scroll-area" style={{ height: "100%" }}>
      {/* Range selector */}
      <div style={{ display: "flex", gap: 6, marginBottom: 12 }}>
        {([7, 30, 90] as Range[]).map(r => (
          <button
            key={r}
            onClick={() => setRange(r)}
            style={{
              padding: "3px 10px",
              fontFamily: "var(--mono)", fontSize: 11,
              border: "1px solid var(--border)",
              borderRadius: 4,
              background: range === r ? "var(--blue)" : "var(--bg2)",
              color:      range === r ? "white"       : "var(--fg2)",
              cursor: "pointer",
            }}
          >
            {r}d
          </button>
        ))}
        <span style={{ fontSize: 10, color: "var(--fg2)", alignSelf: "center", marginLeft: "auto" }}>
          {fmtTokens(totalTokens)} tokens · est. {fmtCost(totalCost)} at API rates
        </span>
      </div>

      {history.length === 0 ? (
        <div style={{ color: "var(--fg2)", fontSize: 11 }}>No history data available</div>
      ) : (
        <>
          {/* Stacked bar chart */}
          <div style={{ height: 200, marginBottom: 12 }}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={chartData} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="var(--bg3)" vertical={false} />
                <XAxis
                  dataKey="date"
                  tick={{ fontSize: 9, fontFamily: "var(--mono)", fill: "var(--fg2)" }}
                  tickLine={false}
                  axisLine={false}
                  interval="preserveStartEnd"
                />
                <YAxis
                  tickFormatter={v => `${v}K`}
                  tick={{ fontSize: 9, fontFamily: "var(--mono)", fill: "var(--fg2)" }}
                  tickLine={false}
                  axisLine={false}
                  width={36}
                />
                <Tooltip
                  formatter={(v: number, name: string) => [`${v}K tokens`, name]}
                  contentStyle={{ fontFamily: "var(--mono)", fontSize: 11, border: "1px solid var(--border)" }}
                />
                <Legend wrapperStyle={{ fontSize: 10, fontFamily: "var(--mono)" }} />
                <Bar dataKey="input"  name="input"        stackId="a" fill="var(--blue)"   radius={[0,0,0,0]} />
                <Bar dataKey="output" name="output"       stackId="a" fill="var(--acc2)"   radius={[0,0,0,0]} />
                <Bar dataKey="cache"  name="cache"        stackId="a" fill="var(--fg2)"    radius={[2,2,0,0]} />
              </BarChart>
            </ResponsiveContainer>
          </div>

          {/* Daily table */}
          <div style={{ borderTop: "1px solid var(--border)", paddingTop: 8 }}>
            <div style={{
              display: "grid",
              gridTemplateColumns: "80px 1fr 1fr 1fr",
              gap: 4,
              fontSize: 10,
              color: "var(--fg2)",
              fontWeight: "bold",
              marginBottom: 4,
            }}>
              <span>date</span>
              <span style={{ textAlign: "right" }}>total</span>
              <span style={{ textAlign: "right" }}>output</span>
              <span style={{ textAlign: "right" }}>est. cost</span>
            </div>
            {[...history].reverse().map(d => (
              <div key={d.period} style={{
                display: "grid",
                gridTemplateColumns: "80px 1fr 1fr 1fr",
                gap: 4,
                fontSize: 10,
                padding: "3px 0",
                borderBottom: "1px solid var(--bg3)",
              }}>
                <span style={{ color: "var(--fg2)" }}>{d.period}</span>
                <span style={{ textAlign: "right" }}>{fmtTokens(d.total_tokens)}</span>
                <span style={{ textAlign: "right", color: "var(--fg2)" }}>{fmtTokens(d.output_tokens)}</span>
                <span style={{ textAlign: "right", color: "var(--fg2)" }}>{fmtCost(d.cost_usd)}</span>
              </div>
            ))}
          </div>

          <div className="disclaimer" style={{ marginTop: 8 }}>
            Cost figures are estimates at public API rates and do not reflect your subscription charges.
          </div>
        </>
      )}
    </div>
  );
}
