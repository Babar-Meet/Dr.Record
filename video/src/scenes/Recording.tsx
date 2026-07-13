import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
} from 'remotion';
import { colors, fonts, radius } from '../theme';

const codeLines = [
  'import React from "react";',
  'import { useState } from "react";',
  '',
  'function App() {',
  '  const [count, setCount] = useState(0);',
  '',
  '  return (',
  '    <div className="app">',
  '      <h1>Hello World</h1>',
  '      <p>Count: {count}</p>',
  '      <button onClick={() => setCount(c => c + 1)}>',
  '        Increment',
  '      </button>',
  '    </div>',
  '  );',
  '}',
  '',
  'export default App;',
];

const codeColor = (line: string) => {
  if (line.includes('import') || line.includes('from')) return '#c792ea';
  if (line.includes('function') || line.includes('return')) return '#82aaff';
  if (line.includes('const') || line.includes('let')) return '#ffcb6b';
  if (line.includes('<') || line.includes('>')) return '#f78c6c';
  if (line.includes('export')) return '#89ddff';
  if (line.trim() === '') return 'transparent';
  return colors.text;
};

export const Recording: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const overlayAppear = spring({ frame: frame - 15, fps, config: { damping: 14, mass: 0.4 } });
  const isSaving = frame > fps * 4;
  const pulse = Math.abs(Math.sin(frame * 0.15));
  const elapsed = Math.max(0, Math.floor((frame - 15) / fps));
  const minutes = String(Math.floor(elapsed / 60)).padStart(2, '0');
  const seconds = String(elapsed % 60).padStart(2, '0');
  const scrollPos = interpolate(frame, [20, 140], [0, -150], { extrapolateRight: 'clamp' });
  const zoomScale = interpolate(frame, [8, 20], [1, 1.06], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const saveNotif = spring({ frame: frame - 130, fps, config: { damping: 12, mass: 0.3 } });

  return (
    <AbsoluteFill style={{ justifyContent: 'center', alignItems: 'center', padding: 60, background: `radial-gradient(ellipse at center, ${colors.bg}, #000)` }}>
      {/* Screen being recorded */}
      <div
        style={{
          width: '88%',
          height: '78%',
          background: '#1e1e2e',
          borderRadius: 10,
          border: `2px solid ${colors.border}`,
          overflow: 'hidden',
          position: 'relative',
          boxShadow: '0 4px 30px rgba(0,0,0,0.5)',
          transform: `scale(${zoomScale})`,
        }}
      >
        {/* Title bar */}
        <div style={{ background: '#181825', padding: '8px 14px', display: 'flex', alignItems: 'center', gap: 10, borderBottom: `1px solid ${colors.border}` }}>
          <div style={{ display: 'flex', gap: 6 }}>
            <div style={{ width: 10, height: 10, borderRadius: '50%', background: colors.red }} />
            <div style={{ width: 10, height: 10, borderRadius: '50%', background: '#ffa500' }} />
            <div style={{ width: 10, height: 10, borderRadius: '50%', background: colors.green }} />
          </div>
          <div style={{ flex: 1, textAlign: 'center', fontFamily: fonts.ui, fontSize: 12, color: colors.textDim }}>
            App.tsx — Visual Studio Code
          </div>
        </div>

        {/* Code */}
        <div style={{ padding: '16px 20px', fontFamily: fonts.mono, fontSize: 14, lineHeight: 1.8, transform: `translateY(${scrollPos}px)` }}>
          {codeLines.map((line, i) => (
            <div key={i} style={{ color: codeColor(line), height: line.trim() === '' ? 18 : undefined }}>
              {line || '\u00A0'}
            </div>
          ))}
        </div>
      </div>

      {/* Floating overlay pill - exact match to app's overlay.html */}
      <div
        style={{
          position: 'absolute',
          top: 84,
          right: 84,
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '6px 14px',
          background: 'rgba(0,0,0,0.8)',
          border: '1px solid rgba(255,255,255,0.08)',
          borderRadius: 20,
          backdropFilter: 'blur(12px)',
          width: 'fit-content',
          transform: `translateY(${(1 - overlayAppear) * -20}px)`,
          opacity: overlayAppear,
        }}
      >
        <div
          style={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: isSaving ? 'orange' : colors.red,
            opacity: isSaving ? 1 : 1 - pulse * 0.6,
          }}
        />
        <span
          style={{
            color: isSaving ? 'orange' : colors.red,
            fontFamily: fonts.ui,
            fontSize: 12,
            fontWeight: 600,
            letterSpacing: '0.5px',
          }}
        >
          {isSaving ? 'SAVING' : 'REC'}
        </span>
        {!isSaving && (
          <span style={{ color: '#fff', fontFamily: fonts.mono, fontSize: 13, minWidth: 38, textAlign: 'center' }}>
            {minutes}:{seconds}
          </span>
        )}
      </div>

      {/* Save notification - matches actual app notification style */}
      {isSaving && saveNotif > 0.3 && (
        <div
          style={{
            position: 'absolute',
            bottom: 100,
            left: '50%',
            transform: 'translateX(-50%)',
            background: '#442026',
            border: `1px solid ${colors.red}`,
            borderRadius: radius,
            padding: '8px 18px',
            fontSize: 12.5,
            color: colors.text,
            fontFamily: fonts.ui,
            opacity: saveNotif,
          }}
        >
          Recording stopped — saving...
        </div>
      )}
    </AbsoluteFill>
  );
};
