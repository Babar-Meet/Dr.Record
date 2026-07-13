import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import { colors, fonts } from '../theme';

const features = [
  {
    icon: '⚡',
    title: 'Zero Setup',
    desc: 'Download and record instantly. No accounts, no config.',
  },
  {
    icon: '🎯',
    title: 'Global Hotkey',
    desc: 'Ctrl+Shift+R — start/stop from anywhere, anytime.',
  },
  {
    icon: '🎨',
    title: 'Crystal Clear',
    desc: 'Up to 144 FPS, lossless quality, any monitor.',
  },
  {
    icon: '🔊',
    title: 'Full Audio',
    desc: 'System audio + mic, live levels, perfect sync.',
  },
];

export const Experience: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  return (
    <AbsoluteFill
      style={{
        justifyContent: 'center',
        alignItems: 'center',
        padding: 60,
      }}
    >
      <div
        style={{
          textAlign: 'center',
          marginBottom: 60,
          opacity: spring({
            frame: frame - 5,
            fps,
            config: { damping: 12, mass: 0.5 },
          }),
        }}
      >
        <h2
          style={{
            fontFamily: fonts.ui,
            fontSize: 42,
            fontWeight: 700,
            color: colors.textBright,
            margin: 0,
          }}
        >
          Everything{" "}
          <span style={{ color: colors.accent }}>Just Works</span>
        </h2>
        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 18,
            color: colors.textDim,
            marginTop: 8,
          }}
        >
          No bloat. No distractions. Just recording.
        </p>
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1fr 1fr',
          gap: 20,
          width: '80%',
          maxWidth: 800,
        }}
      >
        {features.map((feature, i) => {
          const s = spring({
            frame: frame - 20 - i * 8,
            fps,
            config: { damping: 14, mass: 0.5 },
          });

          return (
            <div
              key={feature.title}
              style={{
                background: colors.surface,
                border: `1px solid ${colors.border}`,
                borderRadius: 10,
                padding: '28px 24px',
                transform: `translateY(${(1 - s) * 50}px)`,
                opacity: s,
              }}
            >
              <div
                style={{
                  fontSize: 36,
                  marginBottom: 12,
                }}
              >
                {feature.icon}
              </div>
              <h3
                style={{
                  fontFamily: fonts.ui,
                  fontSize: 20,
                  fontWeight: 600,
                  color: colors.textBright,
                  margin: '0 0 6px 0',
                }}
              >
                {feature.title}
              </h3>
              <p
                style={{
                  fontFamily: fonts.ui,
                  fontSize: 14,
                  color: colors.textDim,
                  margin: 0,
                  lineHeight: 1.5,
                }}
              >
                {feature.desc}
              </p>
            </div>
          );
        })}
      </div>
    </AbsoluteFill>
  );
};
