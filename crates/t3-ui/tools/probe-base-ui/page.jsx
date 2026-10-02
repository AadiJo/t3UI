// Behavior probe page: plain Base UI 1.5 parts (the library under the fork's coss components),
// no styling beyond what geometry checks need. Driven by probe.mjs.
import React from "react";
import { createRoot } from "react-dom/client";
import { Menu } from "@base-ui/react/menu";
import { Select } from "@base-ui/react/select";
import { Tooltip } from "@base-ui/react/tooltip";

const fruits = ["Apple", "Banana", "Blueberry", "Cherry", "Date"];

function ProbeMenu({ id, style }) {
  return (
    <Menu.Root>
      <Menu.Trigger id={id} style={style}>
        Menu
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner sideOffset={4}>
          <Menu.Popup id={`${id}-popup`} style={{ background: "#eee", padding: 4, width: 160 }}>
            {fruits.map((fruit) => (
              <Menu.Item key={fruit} disabled={fruit === "Cherry"} style={{ height: 28 }}>
                {fruit}
              </Menu.Item>
            ))}
            <Menu.SubmenuRoot>
              <Menu.SubmenuTrigger style={{ height: 28 }}>More</Menu.SubmenuTrigger>
              <Menu.Portal>
                <Menu.Positioner>
                  <Menu.Popup id={`${id}-sub`} style={{ background: "#ddd", padding: 4 }}>
                    <Menu.Item style={{ height: 28 }}>Sub one</Menu.Item>
                    <Menu.Item style={{ height: 28 }}>Sub two</Menu.Item>
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.SubmenuRoot>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

function ProbeSelect({ id, style }) {
  return (
    <Select.Root defaultValue="banana">
      <Select.Trigger id={id} style={{ height: 32, width: 200, ...style }}>
        <Select.Value id={`${id}-value`} style={{ paddingLeft: 11 }} />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner alignItemWithTrigger sideOffset={4} align="start">
          <Select.Popup id={`${id}-popup`} style={{ background: "#eee" }}>
            <Select.List style={{ padding: 4 }}>
              {fruits.map((fruit) => (
                <Select.Item
                  key={fruit}
                  value={fruit.toLowerCase()}
                  disabled={fruit === "Cherry"}
                  style={{ height: 28, display: "flex", alignItems: "center", paddingLeft: 8 }}
                >
                  <Select.ItemIndicator style={{ width: 16 }}>✓</Select.ItemIndicator>
                  <Select.ItemText style={{ paddingLeft: 8 }}>{fruit}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

function ProbeTooltip({ id, label }) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger id={id} style={{ width: 60, height: 28 }}>
        {label}
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Positioner sideOffset={4}>
          <Tooltip.Popup id={`${id}-popup`}>{label} tip</Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

function App() {
  return (
    <div style={{ padding: 40, display: "flex", flexDirection: "column", gap: 24 }}>
      <ProbeMenu id="menu" />
      <ProbeSelect id="select" />
      <div style={{ display: "flex" }}>
        <ProbeTooltip id="tip-a" label="A" />
        <ProbeTooltip id="tip-b" label="B" />
      </div>
      <Tooltip.Provider>
        <div style={{ display: "flex" }}>
          <ProbeTooltip id="grp-a" label="GA" />
          <ProbeTooltip id="grp-b" label="GB" />
        </div>
      </Tooltip.Provider>
      <ProbeMenu id="edge-menu" style={{ position: "fixed", right: 8, bottom: 8 }} />
      <ProbeSelect id="edge-select" style={{ position: "fixed", left: 300, bottom: 40 }} />
    </div>
  );
}

createRoot(document.getElementById("root")).render(<App />);
