import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
} from 'remotion';
import { colors, fonts, radius } from '../theme';

const PADDING = 60;
const CONTAINER_W = 540;

const DropdownSequence: React.FC<{
  label: string;
  options: string[];
  selectedIdx: number;
  frame: number;
  openStart: number;
  closeStart: number;
  mono?: boolean;
}> = ({ label, options, selectedIdx, frame, openStart, closeStart, mono }) => {
  const openProgress = interpolate(
    frame,
    [openStart, openStart + 15, closeStart, closeStart + 10],
    [0, 1, 1, 0],
    { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }
  );

  return (
    <div style={{ flex: 1 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, fontWeight: 500, color: colors.textDim, marginBottom: 6, fontFamily: fonts.ui }}>
        {label}
      </div>
      <div style={{ position: 'relative' }}>
        <div
          style={{
            background: colors.inputBg,
            border: `1px solid ${openProgress > 0.05 ? colors.accent : colors.border}`,
            borderRadius: openProgress > 0.05 ? `${radius}px ${radius}px 0 0` : radius,
            padding: '8px 10px',
            color: colors.text,
            fontSize: 13,
            fontFamily: mono ? fonts.mono : fonts.ui,
            textAlign: mono ? 'center' : 'left',
            letterSpacing: mono ? '0.5px' : '0',
            position: 'relative',
            zIndex: 2,
          }}
        >
          <span style={{ float: 'right', color: colors.textDim, fontSize: 10, marginTop: 3 }}>
            {openProgress > 0.05 ? '▲' : '▼'}
          </span>
          {options[selectedIdx]}
        </div>

        <div
          style={{
            position: 'absolute',
            top: '100%',
            left: 0,
            right: 0,
            background: colors.inputBg,
            border: openProgress > 0.05 ? `1px solid ${colors.border}` : 'none',
            borderTop: 'none',
            borderRadius: `0 0 ${radius}px ${radius}px`,
            zIndex: 10,
            overflow: 'hidden',
            maxHeight: openProgress > 0.05 ? interpolate(openProgress, [0.05, 1], [0, 240]) : 0,
            opacity: openProgress,
          }}
        >
          {options.map((opt, i) => (
            <div
              key={opt}
              style={{
                padding: '7px 10px',
                fontSize: 13,
                fontFamily: mono ? fonts.mono : fonts.ui,
                color: i === selectedIdx ? colors.accent : colors.text,
                background: i === selectedIdx ? `${colors.accent}11` : 'transparent',
                borderBottom: i < options.length - 1 ? `1px solid ${colors.border}33` : 'none',
              }}
            >
              {i === selectedIdx && <span style={{ color: colors.accent, marginRight: 6 }}>✓</span>}
              {opt}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

export const SettingsUI: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const containerSpring = spring({ frame, fps, config: { damping: 14, mass: 0.6 } });

  const audioLevel = Math.min(1, Math.max(0.05, Math.sin(frame * 0.07 + 0.5) * 0.4 + 0.45));
  const micLevel = Math.min(1, Math.max(0, Math.sin(frame * 0.1 + 2) * 0.3 + 0.35));
  const levelColor = audioLevel > 0.7 ? colors.red : audioLevel > 0.4 ? '#ffa500' : colors.green;
  const micColor = micLevel > 0.7 ? colors.red : micLevel > 0.4 ? '#ffa500' : colors.green;

  const captureActive = frame > 28 && frame < 50;
  const comboSet = frame > 48;

  return (
    <AbsoluteFill style={{ justifyContent: 'center', alignItems: 'center', padding: PADDING }}>
      <div
        style={{
          background: colors.surface,
          borderRadius: 8,
          padding: '24px 28px',
          width: CONTAINER_W,
          border: `1px solid ${colors.border}`,
          transform: `scale(${containerSpring})`,
          opacity: containerSpring,
        }}
      >
        {/* Header */}
        <div style={{ marginBottom: 20 }}>
          <h1 style={{ fontSize: 20, fontWeight: 600, color: colors.textBright, letterSpacing: '-0.3px', fontFamily: fonts.ui, margin: 0 }}>
            Dr. Record
          </h1>
          <p style={{ color: colors.textDim, fontSize: 12, marginTop: 2, fontFamily: fonts.ui }}>
            Minimal screen recorder for Windows
          </p>
        </div>

        {/* 1. Output Directory */}
        <div style={{ marginBottom: 14, opacity: spring({ frame: frame - 8, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, fontWeight: 500, color: colors.textDim, marginBottom: 6, fontFamily: fonts.ui }}>
            Output Directory
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <div style={{ flex: 1, background: colors.inputBg, border: `1px solid ${colors.border}`, borderRadius: radius, padding: '8px 10px', color: colors.text, fontSize: 13, fontFamily: fonts.ui }}>
              C:\Users\Me\Videos\Dr.Record
            </div>
            <div style={{ padding: '8px 16px', border: `1px solid ${colors.border}`, borderRadius: radius, fontSize: 13, fontWeight: 500, color: colors.text, fontFamily: fonts.ui, background: colors.surface }}>
              Browse
            </div>
          </div>
        </div>

        {/* 2. Hotkey */}
        <div style={{ marginBottom: 14, opacity: spring({ frame: frame - 18, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, fontWeight: 500, color: colors.textDim, marginBottom: 6, fontFamily: fonts.ui }}>
            Start / Stop Hotkey
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <div
              style={{
                flex: 1,
                background: colors.inputBg,
                border: `1px solid ${captureActive || comboSet ? colors.accent : colors.border}`,
                borderRadius: radius,
                padding: '8px 10px',
                color: colors.text,
                fontSize: 13,
                fontFamily: fonts.mono,
                textAlign: 'center',
                letterSpacing: '0.5px',
                boxShadow: captureActive || comboSet ? `0 0 0 1px ${colors.accent}` : 'none',
                transition: 'all 0.2s',
              }}
            >
              {captureActive ? 'Press shortcut...' : comboSet ? 'Ctrl+Shift+R' : 'Ctrl+Shift+Alt+R'}
            </div>
            <div
              style={{
                padding: '8px 16px',
                border: `1px solid ${captureActive ? colors.accent : colors.border}`,
                borderRadius: radius,
                fontSize: 13,
                fontWeight: 500,
                color: captureActive ? colors.accent : colors.text,
                fontFamily: fonts.ui,
                background: captureActive ? `${colors.accent}11` : colors.surface,
                transition: 'all 0.2s',
              }}
            >
              {captureActive ? 'Listening...' : 'Capture'}
            </div>
          </div>
          {comboSet && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 4 }}>
              <span style={{ color: colors.green, fontSize: 11 }}>✓</span>
              <span style={{ color: colors.textDim, fontSize: 11, fontFamily: fonts.ui }}>
                Hotkey set to: Ctrl+Shift+R
              </span>
            </div>
          )}
        </div>

        {/* 3. Source */}
        <div style={{ marginBottom: 12, opacity: spring({ frame: frame - 30, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, fontWeight: 500, color: colors.textDim, marginBottom: 6, fontFamily: fonts.ui }}>
            Recording Source
          </div>
          <div style={{ marginBottom: 8, background: '#111', border: `1px solid ${colors.border}`, borderRadius: 6, height: 110, display: 'flex', justifyContent: 'center', alignItems: 'center', overflow: 'hidden' }}>
            <div style={{ width: '92%', height: '88%', background: `linear-gradient(135deg, #1a1a2e, #16213e, #0f3460)`, borderRadius: 4, display: 'flex', alignItems: 'center', justifyContent: 'center', position: 'relative' }}>
              <div style={{ position: 'absolute', width: '55%', height: 3, background: `${colors.accent}33`, borderRadius: 2, top: '28%' }} />
              <div style={{ position: 'absolute', width: '65%', height: 3, background: `${colors.green}22`, borderRadius: 2, top: '40%' }} />
              <div style={{ position: 'absolute', width: '45%', height: 3, background: `${colors.textDim}22`, borderRadius: 2, top: '52%' }} />
              <div style={{ position: 'absolute', width: '35%', height: 3, background: `${colors.textDim}18`, borderRadius: 2, top: '64%' }} />
              <span style={{ color: colors.textDim, fontSize: 11, fontFamily: fonts.ui }}>Desktop (All Monitors)</span>
            </div>
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <div style={{ flex: 1, background: colors.inputBg, border: `1px solid ${colors.border}`, borderRadius: radius, padding: '8px 10px', color: colors.text, fontSize: 13, fontFamily: fonts.ui, display: 'flex', alignItems: 'center', gap: 6 }}>
              <span>⊞</span>
              <span>All Screens</span>
            </div>
            <div style={{ padding: '8px 10px', border: `1px solid ${colors.border}`, borderRadius: radius, fontSize: 16, color: colors.text, fontFamily: fonts.ui, background: colors.surface, minWidth: 36, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              ↺
            </div>
          </div>
        </div>

        {/* 4. Frame Rate - dropdown opens slowly, then closes */}
        <div style={{ display: 'flex', gap: 12, marginBottom: 14 }}>
          <DropdownSequence
            label="Frame Rate"
            options={['Auto (60 FPS)', '24 FPS', '30 FPS', '60 FPS', '120 FPS', '144 FPS']}
            selectedIdx={0}
            frame={frame}
            openStart={55}
            closeStart={85}
          />
        </div>

        {/* 5. Quality - dropdown opens after framerate closes */}
        <div style={{ display: 'flex', gap: 12, marginBottom: 14 }}>
          <DropdownSequence
            label="Quality"
            options={['Lossless', 'High', 'Medium', 'Low (small file)']}
            selectedIdx={0}
            frame={frame}
            openStart={110}
            closeStart={140}
          />
        </div>

        {/* 6. System Audio */}
        <div style={{ marginBottom: 10, opacity: spring({ frame: frame - 160, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, fontWeight: 500, color: colors.textDim, marginBottom: 6, fontFamily: fonts.ui }}>
            <div style={{ width: 14, height: 14, borderRadius: 3, background: colors.accent, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <div style={{ width: 6, height: 6, borderRadius: 1, background: '#000' }} />
            </div>
            <span>Record System Audio</span>
          </div>
          <div style={{ width: '100%', height: 6, background: colors.inputBg, borderRadius: 3, overflow: 'hidden', border: `1px solid ${colors.border}` }}>
            <div style={{ height: '100%', width: `${audioLevel * 100}%`, background: levelColor, borderRadius: 3 }} />
          </div>
        </div>

        {/* 7. Microphone - dropdown opens after system audio */}
        <div style={{ marginBottom: 14, opacity: spring({ frame: frame - 180, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <DropdownSequence
            label="Microphone"
            options={['None', 'Microphone (Realtek Audio)', 'Headset Microphone', 'Laptop Microphone']}
            selectedIdx={1}
            frame={frame}
            openStart={200}
            closeStart={230}
            wide
          />
          <div style={{ width: '100%', height: 6, marginTop: 8, background: colors.inputBg, borderRadius: 3, overflow: 'hidden', border: `1px solid ${colors.border}` }}>
            <div style={{ height: '100%', width: `${micLevel * 100}%`, background: micColor, borderRadius: 3 }} />
          </div>
        </div>

        {/* 8. Overlay checkbox + display info */}
        <div style={{ marginBottom: 10, opacity: spring({ frame: frame - 250, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, fontWeight: 500, color: colors.textDim, fontFamily: fonts.ui }}>
            <div style={{ width: 14, height: 14, borderRadius: 3, background: colors.accent, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <div style={{ width: 6, height: 6, borderRadius: 1, background: '#000' }} />
            </div>
            <span>Show recording overlay</span>
          </div>
        </div>

        <div style={{ fontSize: 11, color: colors.textDim, textAlign: 'center', marginBottom: 14, padding: 6, background: colors.inputBg, borderRadius: radius, border: `1px solid ${colors.border}`, opacity: spring({ frame: frame - 255, fps, config: { damping: 12, mass: 0.3 } }) }}>
          Display: 1920×1080 @ 60 Hz
        </div>

        {/* 9. Footer */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 16, paddingTop: 14, borderTop: `1px solid ${colors.border}`, opacity: spring({ frame: frame - 265, fps, config: { damping: 12, mass: 0.3 } }) }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12.5, color: colors.textDim, fontFamily: fonts.ui }}>
            <div style={{ width: 7, height: 7, borderRadius: '50%', background: colors.green }} />
            <span>Idle</span>
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <div style={{ padding: '8px 16px', border: `1px solid ${colors.border}`, borderRadius: radius, fontSize: 13, fontWeight: 500, color: colors.text, fontFamily: fonts.ui, background: colors.surface }}>
              Test Record (5s)
            </div>
            <div style={{ padding: '8px 16px', borderRadius: radius, fontSize: 13, fontWeight: 600, color: '#000', fontFamily: fonts.ui, background: colors.accent }}>
              Save & Start
            </div>
          </div>
        </div>
      </div>
    </AbsoluteFill>
  );
};
