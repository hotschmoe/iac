import 'dart:convert';

abstract class ServerMessage {
  factory ServerMessage.fromJson(String raw) {
    final json = jsonDecode(raw) as Map<String, dynamic>;
    final type = json['type'] as String;
    switch (type) {
      case 'tick':
        return TickMessage.fromMap(json);
      case 'welcome':
        return WelcomeMessage.fromMap(json);
      default:
        return UnknownMessage(type, json);
    }
  }
}

class TickMessage implements ServerMessage {
  final int tick;
  final Map<String, dynamic> data;
  TickMessage({required this.tick, required this.data});
  factory TickMessage.fromMap(Map<String, dynamic> m) =>
      TickMessage(tick: m['tick'] as int, data: m);
}

class WelcomeMessage implements ServerMessage {
  final int tick;
  final String playerId;
  WelcomeMessage({required this.tick, required this.playerId});
  factory WelcomeMessage.fromMap(Map<String, dynamic> m) =>
      WelcomeMessage(tick: m['tick'] as int, playerId: m['player_id'] as String);
}

class UnknownMessage implements ServerMessage {
  final String type;
  final Map<String, dynamic> data;
  UnknownMessage(this.type, this.data);
}

abstract class ClientMessage {
  String toJson();
}

class MoveCommand implements ClientMessage {
  final int direction;
  MoveCommand(this.direction);
  @override
  String toJson() => jsonEncode({'type': 'move', 'direction': direction});
}

class HarvestCommand implements ClientMessage {
  @override
  String toJson() => jsonEncode({'type': 'harvest'});
}

class AttackCommand implements ClientMessage {
  @override
  String toJson() => jsonEncode({'type': 'attack'});
}
