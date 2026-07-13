import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import { colors, fonts, radius } from '../theme';

const PADDING = 60;
const CONTAINER_W = 520;

const SettingRow: React.FC<{
  label: string;
  value: string;
  mono?: boolean;
  delay?: number;
}> = ({ label, value, mono, delay = 0 }) => {
  const { fps } = useVideoConfig();
  const frame = useCurrentFrame();

  const s = spring({
    frame: frame - delay,
    fps,
    config: { damping: 14, mass: 0.4 },
  });

  return (
    <div
      style={{
        marginBottom: 14,
        opacity: s,
        transform: `translateX(${(1 - s) * 40}px)`,
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          fontSize: 12,
          fontWeight: 500,
          color: colors.textDim,
          marginBottom: 6,
          fontFamily: fonts.ui,
        }}
      >
        {label}
      </div>
      <div
        style={{
          display: 'flex',
          gap: 6,
        }}
      >
        <div
          style={{
            flex: 1,
            background: colors.inputBg,
            border: `1px solid ${colors.border}`,
            borderRadius: radius,
            padding: '8px 10px',
            color: colors.text,
            fontSize: 13,
            fontFamily: mono ? fonts.mono : fonts.ui,
            textAlign: mono ? 'center' : 'left',
            letterSpacing: mono ? '0.5px' : '0',
          }}
        >
          {value}
        </div>
        <div
          style={{
            padding: '8px 16px',
            border: `1px solid ${colors.border}`,
            borderRadius: radius,
            fontSize: 13,
            fontWeight: 500,
            color: colors.text,
            fontFamily: fonts.ui,
            background: colors.surface,
            cursor: 'pointer',
          }}
        >
          {label === 'Start / Stop Hotkey' ? 'Capture' : 'Browse'}
        </div>
      </div>
    </div>
  );
};

export const SettingsUI: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const containerSpring = spring({
    frame,
    fps,
    config: { damping: 14, mass: 0.6 },
  });

  const audioLevel = Math.min(1, Math.max(0.1, Math.sin(frame * 0.08) * 0.5 + 0.5));
  const micLevel = Math.min(1, Math.max(0, Math.sin(frame * 0.12 + 1) * 0.3 + 0.4));

  const levelColor =
    audioLevel > 0.7
      ? colors.red
      : audioLevel > 0.4
        ? '#ffa500'
        : colors.green;

  const micColor =
    micLevel > 0.7
      ? colors.red
      : micLevel > 0.4
        ? '#ffa500'
        : colors.green;

  const isRecording = frame > fps * 2.5 && frame < fps * 4;

  return (
    <AbsoluteFill
      style={{
        justifyContent: 'center',
        alignItems: 'center',
        padding: PADDING,
      }}
    >
      <div
        style={{
          background: colors.surface,
          borderRadius: 8,
          padding: '24px 28px',
          width: CONTAINER_W,
          border: `1px solid ${isRecording ? colors.red : colors.border}`,
          boxShadow: isRecording
            ? `0 0 30px ${colors.red}22`
            : 'none',
          transform: `scale(${containerSpring})`,
          opacity: containerSpring,
          transition: 'border-color 0.3s, box-shadow 0.3s',
        }}
      >
        {/* Header */}
        <div style={{ marginBottom: 24 }}>
          <h1
            style={{
              fontSize: 20,
              fontWeight: 600,
              color: colors.textBright,
              letterSpacing: '-0.3px',
              fontFamily: fonts.ui,
              margin: 0,
            }}
          >
            Dr. Record
          </h1>
          <p
            style={{
              color: colors.textDim,
              fontSize: 12,
              marginTop: 2,
              fontFamily: fonts.ui,
            }}
          >
            Screen recorder for Windows
          </p>
        </div>

        <SettingRow label="Output Directory" value="C:\Users\Me\Videos" delay={5} />
        <SettingRow
          label="Start / Stop Hotkey"
          value="Ctrl+Shift+R"
          mono
          delay={15}
        />

        {/* Recording Source */}
        <div
          style={{
            marginBottom: 14,
            opacity: spring({
              frame: frame - 25,
              fps,
              config: { damping: 14, mass: 0.4 },
            }),
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              fontSize: 12,
              fontWeight: 500,
              color: colors.textDim,
              marginBottom: 6,
              fontFamily: fonts.ui,
            }}
          >
            Recording Source
          </div>
          <div
            style={{
              marginBottom: 10,
              background: '#111',
              border: `1px solid ${colors.border}`,
              borderRadius: 6,
              height: 120,
              display: 'flex',
              justifyContent: 'center',
              alignItems: 'center',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                width: '90%',
                height: '90%',
                background: `linear-gradient(135deg, #1a1a2e, #16213e, #0f3460)`,
                borderRadius: 4,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                position: 'relative',
                overflow: 'hidden',
              }}
            >
              {/* Animated screen content */}
              <div
                style={{
                  position: 'absolute',
                  width: '60%',
                  height: 4,
                  background: `${colors.accent}33`,
                  borderRadius: 2,
                  top: '30%',
                }}
              />
              <div
                style={{
                  position: 'absolute',
                  width: '40%',
                  height: 4,
                  background: `${colors.green}22`,
                  borderRadius: 2,
                  top: '42%',
                }}
              />
              <div
                style={{
                  position: 'absolute',
                  width: '50%',
                  height: 4,
                  background: `${colors.textDim}22`,
                  borderRadius: 2,
                  top: '54%',
                }}
              />
              <span
                style={{
                  color: colors.textDim,
                  fontSize: 11,
                  fontFamily: fonts.ui,
                }}
              >
                Live preview
              </span>
            </div>
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <div
              style={{
                flex: 1,
                background: colors.inputBg,
                border: `1px solid ${colors.border}`,
                borderRadius: radius,
                padding: '8px 10px',
                color: colors.text,
                fontSize: 13,
                fontFamily: fonts.ui,
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                gap: 6,
              }}
            >
              <span>⊞</span>
              <span>All Screens</span>
            </div>
            <div
              style={{
                padding: '8px 10px',
                border: `1px solid ${colors.border}`,
                borderRadius: radius,
                fontSize: 16,
                color: colors.text,
                fontFamily: fonts.ui,
                background: colors.surface,
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                minWidth: 36,
              }}
            >
              ↺
            </div>
          </div>
        </div>

        {/* Frame rate + Quality row */}
        <div
          style={{
            display: 'flex',
            gap: 12,
            marginBottom: 14,
            opacity: spring({
              frame: frame - 35,
              fps,
              config: { damping: 14, mass: 0.4 },
            }),
          }}
        >
          {[
            { label: 'Frame Rate', value: '60 FPS' },
            { label: 'Quality', value: 'High' },
          ].map((item) => (
            <div key={item.label} style={{ flex: 1 }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  fontSize: 12,
                  fontWeight: 500,
                  color: colors.textDim,
                  marginBottom: 6,
                  fontFamily: fonts.ui,
                }}
              >
                {item.label}
              </div>
              <div
                style={{
                  background: colors.inputBg,
                  border: `1px solid ${colors.border}`,
                  borderRadius: radius,
                  padding: '8px 10px',
                  color: colors.text,
                  fontSize: 13,
                  fontFamily: fonts.ui,
                  cursor: 'pointer',
                }}
              >
                {item.value}
              </div>
            </div>
          ))}
        </div>

        {/* Audio settings */}
        <div
          style={{
            marginBottom: 10,
            opacity: spring({
              frame: frame - 45,
              fps,
              config: { damping: 14, mass: 0.4 },
            }),
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              fontSize: 12,
              fontWeight: 500,
              color: colors.textDim,
              marginBottom: 6,
              fontFamily: fonts.ui,
            }}
          >
            <div
              style={{
                width: 14,
                height: 14,
                borderRadius: 3,
                background: colors.accent,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <div
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 1,
                  background: '#000',
                }}
              />
            </div>
            <span>Record System Audio</span>
          </div>
          <div
            style={{
              width: '100%',
              height: 6,
              background: colors.inputBg,
              borderRadius: 3,
              overflow: 'hidden',
              border: `1px solid ${colors.border}`,
            }}
          >
            <div
              style={{
                height: '100%',
                width: `${audioLevel * 100}%`,
                background: levelColor,
                borderRadius: 3,
                transition: 'width 0.05s, background 0.1s',
              }}
            />
          </div>
        </div>

        <div
          style={{
            marginBottom: 14,
            opacity: spring({
              frame: frame - 50,
              fps,
              config: { damping: 14, mass: 0.4 },
            }),
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              fontSize: 12,
              fontWeight: 500,
              color: colors.textDim,
              marginBottom: 6,
              fontFamily: fonts.ui,
            }}
          >
            Microphone
          </div>
          <div style={{ display: 'flex', gap: 6, marginBottom: 8 }}>
            <div
              style={{
                flex: 1,
                background: colors.inputBg,
                border: `1px solid ${colors.border}`,
                borderRadius: radius,
                padding: '8px 10px',
                color: colors.text,
                fontSize: 13,
                fontFamily: fonts.ui,
                cursor: 'pointer',
              }}
            >
              Microphone (Realtek Audio)
            </div>
            <div
              style={{
                padding: '8px 10px',
                border: `1px solid ${colors.border}`,
                borderRadius: radius,
                fontSize: 16,
                color: colors.text,
                fontFamily: fonts.ui,
                background: colors.surface,
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                minWidth: 36,
              }}
            >
              ↺
            </div>
          </div>
          <div
            style={{
              width: '100%',
              height: 6,
              background: colors.inputBg,
              borderRadius: 3,
              overflow: 'hidden',
              border: `1px solid ${colors.border}`,
            }}
          >
            <div
              style={{
                height: '100%',
                width: `${micLevel * 100}%`,
                background: micColor,
                borderRadius: 3,
                transition: 'width 0.05s, background 0.1s',
              }}
            />
          </div>
        </div>

        {/* Footer */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginTop: 20,
            paddingTop: 16,
            borderTop: `1px solid ${colors.border}`,
            opacity: spring({
              frame: frame - 60,
              fps,
              config: { damping: 14, mass: 0.4 },
            }),
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              fontSize: 12.5,
              color: colors.textDim,
              fontFamily: fonts.ui,
            }}
          >
            <div
              style={{
                width: 7,
                height: 7,
                borderRadius: '50%',
                background: isRecording ? colors.red : colors.green,
                animation: isRecording ? 'pulse 1s infinite' : 'none',
              }}
            />
            <span>{isRecording ? 'Recording...' : 'Idle'}</span>
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            <div
              style={{
                padding: '8px 16px',
                border: `1px solid ${colors.border}`,
                borderRadius: radius,
                fontSize: 13,
                fontWeight: 500,
                color: colors.text,
                fontFamily: fonts.ui,
                background: colors.surface,
                cursor: 'pointer',
              }}
            >
              Test Record (5s)
            </div>
            <div
              style={{
                padding: '8px 16px',
                border: `1px solid ${colors.accent}`,
                borderRadius: radius,
                fontSize: 13,
                fontWeight: 600,
                color: '#000',
                fontFamily: fonts.ui,
                background: colors.accent,
                cursor: 'pointer',
              }}
            >
              {isRecording ? 'Stop' : 'Save & Start'}
            </div>
          </div>
        </div>
      </div>
    </AbsoluteFill>
  );
};
