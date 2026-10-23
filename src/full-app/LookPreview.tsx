import { createContext, useContext } from 'react';
import { Picker } from './Picker';
import { DEFAULT_LOOK, type Look } from './lookModel';

export const LookContext = createContext({ look: DEFAULT_LOOK, changeLook: (_look: Look) => {} });

// shortcut: temporary owner comparison in all builds; remove after the final look is chosen.
export function LookPreview() {
  const { look, changeLook } = useContext(LookContext);
  return <div className="setting-row look-preview-row">
    <div className="setting-copy"><strong>Look (preview)</strong><p>Temporary appearance comparison.</p></div>
    <Picker label="Look (preview)" value={look} onChange={value => changeLook(value as Look)} options={[
      { value: 'glass-context', label: 'Glass · contextual' },
      { value: 'glass-neutral', label: 'Glass · neutral' },
      { value: 'solid-accent', label: 'Solid + accent' },
    ]} />
  </div>;
}
