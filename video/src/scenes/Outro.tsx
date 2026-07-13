import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import { colors, fonts } from '../theme';

export const Outro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const titleSpring = spring({
    frame,
    fps,
    config: { damping: 12, mass: 0.5 },
  });

  const subtitleSpring = spring({
    frame: frame - 20,
    fps,
    config: { damping: 12, mass: 0.5 },
  });

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
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 24,
        }}
      >
        <div
          style={{
            width: 72,
            height: 72,
            borderRadius: 16,
            background: `linear-gradient(135deg, ${colors.accent}, ${colors.accentHover})`,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: 28,
            fontWeight: 700,
            color: '#000',
            fontFamily: fonts.ui,
            opacity: titleSpring,
            transform: `scale(${titleSpring})`,
          }}
        >
          DR
        </div>

        <h2
          style={{
            fontFamily: fonts.ui,
            fontSize: 40,
            fontWeight: 700,
            color: colors.textBright,
            textAlign: 'center',
            margin: 0,
            opacity: titleSpring,
            transform: `translateY(${(1 - titleSpring) * 40}px)`,
          }}
        >
          Dr. Record
        </h2>

        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 18,
            color: colors.accent,
            textAlign: 'center',
            margin: 0,
            fontWeight: 500,
            opacity: subtitleSpring,
            transform: `translateY(${(1 - subtitleSpring) * 30}px)`,
          }}
        >
          Zero BS Screen Recorder
        </p>

        <div
          style={{
            display: 'flex',
            gap: 16,
            marginTop: 20,
            opacity: subtitleSpring,
          }}
        >
          <div
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              background: colors.green,
            }}
          />
          <div
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              background: colors.accent,
            }}
          />
          <div
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              background: colors.red,
            }}
          />
        </div>

        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 14,
            color: colors.textDim,
            textAlign: 'center',
            marginTop: 40,
            opacity: spring({
              frame: frame - 40,
              fps,
              config: { damping: 12, mass: 0.5 },
            }),
          }}
        >
          Because how you record matters
        </p>
      </div>
    </AbsoluteFill>
  );
};
