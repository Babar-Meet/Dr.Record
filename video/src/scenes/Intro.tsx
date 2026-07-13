import { AbsoluteFill, spring, useCurrentFrame, useVideoConfig, Img, staticFile } from 'remotion';
import { colors, fonts } from '../theme';

export const Intro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const titleSpring = spring({
    frame,
    fps,
    config: { damping: 12, mass: 0.5 },
  });

  const subtitleSpring = spring({
    frame: frame - 15,
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
          gap: 40,
          transform: `translateY(${(1 - titleSpring) * 60}px)`,
          opacity: titleSpring,
        }}
      >
        <Img
          src={staticFile('/icon.png')}
          style={{
            width: 80,
            height: 80,
            borderRadius: 16,
            boxShadow: `0 0 40px ${colors.accent}33`,
          }}
        />

        <h1
          style={{
            fontFamily: fonts.ui,
            fontSize: 52,
            fontWeight: 700,
            color: colors.textBright,
            textAlign: 'center',
            lineHeight: 1.2,
            letterSpacing: '-0.5px',
            margin: 0,
          }}
        >
          User Experience
          <br />
          <span style={{ color: colors.accent }}>is Everything</span>
        </h1>

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            opacity: subtitleSpring,
            transform: `translateY(${(1 - subtitleSpring) * 30}px)`,
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
          <span
            style={{
              fontFamily: fonts.ui,
              fontSize: 20,
              color: colors.textDim,
              fontWeight: 400,
            }}
          >
            Dr. Record — Minimal Screen Recorder
          </span>
        </div>
      </div>

      <div
        style={{
          position: 'absolute',
          bottom: 80,
          left: '50%',
          transform: 'translateX(-50%)',
          display: 'flex',
          gap: 8,
        }}
      >
        {[0, 1, 2, 3].map((i) => (
          <div
            key={i}
            style={{
              width: 40,
              height: 3,
              borderRadius: 2,
              background: frame > 60 + i * 10 ? colors.accent : colors.border,
            }}
          />
        ))}
      </div>
    </AbsoluteFill>
  );
};
